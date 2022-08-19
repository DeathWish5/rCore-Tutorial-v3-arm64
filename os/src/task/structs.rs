use alloc::string::String;
use alloc::sync::{Arc, Weak};
use alloc::{boxed::Box, vec::Vec};
use core::sync::atomic::{AtomicI32, AtomicU8, AtomicUsize, Ordering};

use super::manager::{TaskLockedCell, TASK_MANAGER, TASK_MAP};
use super::percpu::PerCpu;
use super::signal::{SignalActions, SignalFlags, MAX_SIG};
use super::switch::TaskContext;
use crate::config::KERNEL_STACK_SIZE;
use crate::fs::{open_file, OpenFlags};
use crate::fs::{File, Stdin, Stdout};
use crate::mm::{MemorySet, PhysAddr};
use crate::sync::{LazyInit, Mutex};
use crate::trap::TrapFrame;

pub static ROOT_TASK: LazyInit<Arc<Task>> = LazyInit::new();

enum EntryState {
    Kernel { pc: usize, arg: usize },
    User(Box<TrapFrame>),
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct TaskId(usize);

#[repr(u8)]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum TaskState {
    Ready = 1,
    Running = 2,
    Zombie = 3,
}

pub struct Task {
    id: TaskId,
    is_kernel: bool,
    state: AtomicU8,
    entry: EntryState,
    exit_code: AtomicI32,

    kstack: Stack<KERNEL_STACK_SIZE>,
    ctx: TaskLockedCell<TaskContext>,

    vm: Mutex<Option<MemorySet>>,
    pub parent: Mutex<Weak<Task>>,
    pub children: Mutex<Vec<Arc<Task>>>,
    pub fd_table: Mutex<Vec<Option<Arc<dyn File + Send + Sync>>>>,
    pub signal: Mutex<SignalInner>,
}

pub struct SignalInner {
    pub signals: SignalFlags,
    pub signal_mask: SignalFlags,
    // the signal which is being handling
    pub handling_sig: Option<usize>,
    // Signal actions
    pub signal_actions: SignalActions,
    // if the task is killed
    pub killed: bool,
    // if the task is frozen by a signal
    pub frozen: bool,
    pub mask_backup: Option<SignalFlags>,
    pub trapframe_backup: Option<TrapFrame>,
}

impl TaskId {
    const IDLE_TASK_ID: Self = Self(0);

    fn alloc() -> Self {
        static NEXT_PID: AtomicUsize = AtomicUsize::new(1);
        Self(NEXT_PID.fetch_add(1, Ordering::AcqRel))
    }

    pub const fn as_usize(&self) -> usize {
        self.0
    }
}

impl From<usize> for TaskId {
    fn from(pid: usize) -> Self {
        Self(pid)
    }
}

impl From<u8> for TaskState {
    fn from(state: u8) -> Self {
        match state {
            1 => Self::Ready,
            2 => Self::Running,
            3 => Self::Zombie,
            _ => panic!("invalid task state: {}", state),
        }
    }
}

impl Task {
    fn new_common(id: TaskId, is_kernel: bool) -> Self {
        Self {
            id,
            is_kernel,
            state: AtomicU8::new(TaskState::Ready as u8),
            entry: EntryState::Kernel { pc: 0, arg: 0 },
            exit_code: AtomicI32::new(0),

            kstack: Stack::default(),
            ctx: TaskLockedCell::new(TaskContext::default()),

            vm: Mutex::new(None),
            parent: Mutex::new(Weak::default()),
            children: Mutex::new(Vec::new()),
            fd_table: Mutex::new(alloc::vec![
                // 0 -> stdin
                Some(Arc::new(Stdin)),
                // 1 -> stdout
                Some(Arc::new(Stdout)),
                // 2 -> stderr
                Some(Arc::new(Stdout)),
            ]),
            signal: Mutex::new(SignalInner {
                signals: SignalFlags::empty(),
                signal_mask: SignalFlags::empty(),
                handling_sig: None,
                signal_actions: SignalActions::default(),
                killed: false,
                frozen: false,
                mask_backup: None,
                trapframe_backup: None,
            }),
        }
    }

    pub fn add_child(self: &Arc<Self>, child: &Arc<Task>) {
        *child.parent.lock() = Arc::downgrade(self);
        self.children.lock().push(child.clone());
    }

    pub fn new_idle() -> Arc<Self> {
        let t = Self::new_common(TaskId::IDLE_TASK_ID, true);
        t.set_state(TaskState::Running);
        Arc::new(t)
    }

    pub fn new_kernel(entry: fn(usize) -> usize, arg: usize) -> Arc<Self> {
        let mut t = Self::new_common(TaskId::alloc(), true);
        t.entry = EntryState::Kernel {
            pc: entry as usize,
            arg,
        };
        t.ctx
            .get_mut()
            .init(task_entry as _, t.kstack.top(), PhysAddr::new(0));

        let t = Arc::new(t);
        if !t.is_root() {
            ROOT_TASK.add_child(&t);
        }
        t
    }

    pub fn new_user(path: &str) -> Arc<Self> {
        let elf_data = open_file(path, OpenFlags::RDONLY).expect("No such user program");
        let mut vm = MemorySet::new();
        let (entry, ustack_top) = vm.load_user(elf_data.read_all().as_slice());

        let mut t = Self::new_common(TaskId::alloc(), false);
        t.entry = EntryState::User(Box::new(TrapFrame::new_user(entry, ustack_top)));
        t.ctx
            .get_mut()
            .init(task_entry as _, t.kstack.top(), vm.page_table_root());
        *t.vm.lock() = Some(vm);

        let t = Arc::new(t);
        ROOT_TASK.add_child(&t);
        t
    }

    pub fn new_fork(self: &Arc<Self>, tf: &TrapFrame) -> Arc<Self> {
        assert!(!self.is_kernel_task());
        let mut t = Self::new_common(TaskId::alloc(), false);
        let vm = self.vm.lock().clone().unwrap();
        t.entry = EntryState::User(Box::new(tf.new_fork()));
        t.ctx
            .get_mut()
            .init(task_entry as _, t.kstack.top(), vm.page_table_root());
        *t.vm.lock() = Some(vm);
        // copy fd table
        let mut new_fd_table: Vec<Option<Arc<dyn File + Send + Sync>>> = Vec::new();
        for fd in self.fd_table.lock().iter() {
            if let Some(file) = fd {
                new_fd_table.push(Some(file.clone()));
            } else {
                new_fd_table.push(None);
            }
        }
        *t.fd_table.lock() = new_fd_table;
        let t = Arc::new(t);
        self.add_child(&t);
        t
    }

    pub fn alloc_fd(self: &Arc<Self>, file: Option<Arc<dyn File + Send + Sync>>) -> usize {
        let mut fd_table = self.fd_table.lock();
        let fd = if let Some(fd) = (0..fd_table.len()).find(|fd| fd_table[*fd].is_none()) {
            fd
        } else {
            fd_table.push(None);
            fd_table.len() - 1
        };
        if let Some(file) = file {
            fd_table[fd] = Some(file);
        }
        fd
    }

    pub fn set_singal(self: &Arc<Self>, signal: SignalFlags) {
        let mut inner = self.signal.lock();
        inner.signals |= signal;
    }

    pub fn singal_metadata(self: &Arc<Self>) -> (SignalFlags, Option<SignalFlags>) {
        let inner = self.signal.lock();
        let masked_signal = inner.signals & inner.signal_mask;
        if let Some(sig) = inner.handling_sig {
            let action_mask = inner.signal_actions.table[sig].mask;
            (masked_signal, Some(action_mask))
        } else {
            (masked_signal, None)
        }
    }

    fn kernel_signal_handler(self: &Arc<Self>, signal: SignalFlags) {
        let mut inner = self.signal.lock();
        match signal {
            SignalFlags::SIGSTOP => {
                inner.frozen = true;
                inner.signals ^= SignalFlags::SIGSTOP;
            }
            SignalFlags::SIGCONT => {
                if inner.signals.contains(SignalFlags::SIGCONT) {
                    inner.signals ^= SignalFlags::SIGCONT;
                    inner.frozen = false;
                }
            }
            _ => {
                info!(
                    "[K] call_kernel_signal_handler:: current task sigflag {:?}",
                    inner.signals
                );
                inner.killed = true;
            }
        }
    }

    fn user_signal_handler(self: &Arc<Self>, sig: usize, tf: &mut TrapFrame) {
        let mut inner = self.signal.lock();
        let handler = inner.signal_actions.table[sig].handler;
        if handler != 0 {
            // backup mask
            inner.mask_backup = Some(inner.signal_mask);
            // change current mask
            inner.signal_mask = inner.signal_actions.table[sig].mask;
            // handle flag
            inner.handling_sig = Some(sig);
            inner.signals ^= SignalFlags::from_bits(1 << sig).unwrap();
            // backup and modify trapframe
            inner.trapframe_backup = Some(*tf);
            // set entry point
            tf.elr = handler as _;
            // set arg0
            tf.r[0] = sig as _;
        } else {
            // default action
            info!("[K] task/call_user_signal_handler: default action: ignore it or kill process");
        }
    }

    pub fn is_kernel_task(&self) -> bool {
        self.is_kernel
    }

    pub const fn is_root(&self) -> bool {
        self.id.as_usize() == 1
    }

    pub const fn is_idle(&self) -> bool {
        self.id.as_usize() == 0
    }

    pub fn pid(&self) -> TaskId {
        self.id
    }

    pub fn state(&self) -> TaskState {
        self.state.load(Ordering::SeqCst).into()
    }

    pub fn set_state(&self, state: TaskState) {
        self.state.store(state as u8, Ordering::SeqCst)
    }

    pub fn exit_code(&self) -> i32 {
        self.exit_code.load(Ordering::SeqCst)
    }

    pub fn set_exit_code(&self, exit_code: i32) {
        self.exit_code.store(exit_code, Ordering::SeqCst)
    }

    pub fn context(&self) -> &TaskLockedCell<TaskContext> {
        &self.ctx
    }

    pub fn traverse(self: &Arc<Self>, func: &impl Fn(&Arc<Task>)) {
        func(self);
        for c in self.children.lock().iter() {
            c.traverse(func);
        }
    }
}

fn task_entry() -> ! {
    // release the lock that was implicitly held across the reschedule
    unsafe { TASK_MANAGER.force_unlock() };
    crate::arch::enable_irqs();
    let task = CurrentTask::get();
    match &task.entry {
        EntryState::Kernel { pc, arg } => {
            let entry: fn(usize) -> i32 = unsafe { core::mem::transmute(*pc) };
            let ret = entry(*arg);
            task.exit(ret as _);
        }
        EntryState::User(tf) => {
            unsafe { tf.exec(task.kstack.top()) };
        }
    }
}

fn push_stack<T>(stack: usize, val: T) -> usize {
    let stack = unsafe { (stack as *mut T).sub(1) };
    unsafe { *stack = val };
    stack as usize
}

fn push_c_str(stack: usize, val: &String) -> usize {
    let len = val.len() + 1;
    let stack = unsafe { core::slice::from_raw_parts_mut((stack - len) as *mut u8, len) };
    for (idx, c) in val.as_bytes().iter().enumerate() {
        stack[idx] = *c;
    }
    stack[len - 1] = 0;
    stack.as_ptr() as usize
}

pub struct CurrentTask<'a>(pub &'a Arc<Task>);

impl<'a> CurrentTask<'a> {
    pub fn get() -> Self {
        Self(PerCpu::current().current_task())
    }

    pub fn yield_now(&self) {
        TASK_MANAGER.lock().yield_current(self)
    }

    pub fn exit(&self, exit_code: i32) -> ! {
        *self.vm.lock() = None; // drop memory set before lock
        TASK_MANAGER.lock().exit_current(self, exit_code)
    }

    pub fn exec(&self, path: &str, args: Vec<String>, tf: &mut TrapFrame) -> isize {
        assert!(!self.is_kernel_task());
        if let Some(elf_data) = open_file(path, OpenFlags::RDONLY) {
            let mut vm = self.vm.lock();
            let vm = vm.get_or_insert(MemorySet::new());
            vm.clear();
            crate::arch::flush_tlb_all();
            let (entry, mut ustack_top) = vm.load_user(elf_data.read_all().as_slice());
            let argc = args.len();
            let mut argv_vec = Vec::<usize>::new();
            for arg in args.iter().rev() {
                ustack_top = push_c_str(ustack_top, arg);
                argv_vec.push(ustack_top);
            }
            ustack_top = ustack_top & !0xF; // 16 bytes aligned
            for arg in argv_vec {
                ustack_top = push_stack(ustack_top, arg);
            }
            let argv_base = ustack_top;
            *tf = TrapFrame::new_user_arg(entry, ustack_top, argc as _, argv_base as _);
            argc as isize
        } else {
            -1
        }
    }

    pub fn waitpid(&self, pid: isize, exit_code: &mut i32) -> isize {
        let mut children = self.children.lock();
        let mut found_pid = false;
        for (idx, t) in children.iter().enumerate() {
            if pid == -1 || t.pid().as_usize() == pid as usize {
                found_pid = true;
                if t.state() == TaskState::Zombie {
                    let child = children.remove(idx);
                    TASK_MAP.lock().remove(&child.pid().as_usize());
                    assert_eq!(Arc::strong_count(&child), 1);
                    *exit_code = child.exit_code();
                    return child.pid().as_usize() as isize;
                }
            }
        }
        if found_pid {
            -2
        } else {
            -1
        }
    }

    fn check_pending_signals(&self, tf: &mut TrapFrame) -> Option<SignalFlags> {
        for sig in 0..(MAX_SIG + 1) {
            let (masked_singal, handling_sig_mask) = self.singal_metadata();
            let signal = SignalFlags::from_bits(1 << sig).unwrap();
            if masked_singal.contains(signal)
                && (handling_sig_mask == None || handling_sig_mask.unwrap().contains(signal))
            {
                if SignalFlags::KERNEL_SIGNAL.contains(signal) {
                    // signal is a kernel signal
                    self.kernel_signal_handler(signal);
                } else {
                    // signal is a user signal
                    self.user_signal_handler(sig, tf);
                }
                return Some(signal);
            }
        }
        None
    }

    pub fn handle_signals(&self, tf: &mut TrapFrame) -> Option<(i32, &'static str)> {
        loop {
            self.check_pending_signals(tf);
            let inner = self.signal.lock();
            let frozen_flag = inner.frozen;
            let killed_flag = inner.killed;
            drop(inner);
            if (!frozen_flag) || killed_flag {
                break;
            }
            self.yield_now();
        }
        self.signal.lock().signals.check_error()
    }
}

impl<'a> core::ops::Deref for CurrentTask<'a> {
    type Target = Arc<Task>;
    fn deref(&self) -> &Self::Target {
        self.0
    }
}

struct Stack<const N: usize>(Box<[u8]>);

impl<const N: usize> Stack<N> {
    pub fn default() -> Self {
        Self(Box::from(alloc::vec![0; N]))
    }

    pub fn top(&self) -> usize {
        self.0.as_ptr_range().end as usize
    }
}
