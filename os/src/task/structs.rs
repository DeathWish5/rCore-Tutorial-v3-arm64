use crate::config::{USER_STACK_SIZE, USER_STACK_TOP};
use alloc::string::String;
use alloc::sync::{Arc, Weak};
use alloc::{boxed::Box, collections::BTreeMap, vec::Vec};
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU8, AtomicUsize, Ordering};

use super::manager::{TaskLockedCell, PROC_MAP, TASK_MANAGER};
use super::percpu::PerCpu;
use super::signal::{SignalActions, SignalFlags, MAX_SIG};
use super::switch::TaskContext;
use crate::config::KERNEL_STACK_SIZE;
use crate::fs::{open_file, OpenFlags};
use crate::fs::{File, Stdin, Stdout};
use crate::mm::{MapArea, MemFlags, MemorySet, PhysAddr, VirtAddr, PAGE_SIZE};
use crate::sync::{LazyInit, Mutex};
use crate::trap::TrapFrame;

pub static ROOT_PROC: LazyInit<Arc<Process>> = LazyInit::new();

enum EntryState {
    Kernel { pc: usize, arg: usize },
    User(Box<TrapFrame>),
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ProcId(usize);

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct TaskId(usize);

#[repr(u8)]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum TaskState {
    Ready = 1,
    Running = 2,
    Zombie = 3,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ProcState {
    Normal = 1,
    Stop = 2,
    Zombie = 3,
}

pub struct Task {
    tid: TaskId,
    is_kernel: bool,
    process: Weak<Process>,
    state: AtomicU8,
    entry: EntryState,
    exit_code: AtomicI32,
    kstack: Stack<KERNEL_STACK_SIZE>,
    ctx: TaskLockedCell<TaskContext>,
    pub signal: Mutex<SignalInner>,
}

pub struct Process {
    id: ProcId,
    is_kernel: bool,
    state: AtomicU8,
    exit_code: AtomicI32,
    pub vm: Mutex<Option<MemorySet>>,

    pub tasks: Mutex<BTreeMap<usize, Arc<Task>>>,
    tid_allocator: IdAllocator,

    pub parent: Mutex<Weak<Process>>,
    pub children: Mutex<Vec<Arc<Process>>>,
    pub fd_table: Mutex<Vec<Option<Arc<dyn File + Send + Sync>>>>,
    pub signal_actions: Mutex<SignalActions>,
}

pub struct SignalInner {
    pub signals: SignalFlags,
    pub signal_mask: SignalFlags,
    // the signal which is being handling
    pub handling_sig: Option<usize>,
    // if the task is killed
    pub killed: bool,
    // if the task is frozen by a signal
    pub frozen: bool,
    pub mask_backup: Option<SignalFlags>,
    pub trapframe_backup: Option<TrapFrame>,
}

struct IdAllocator {
    id: AtomicUsize,
}

impl IdAllocator {
    pub const fn zero() -> Self {
        Self {
            id: AtomicUsize::new(0),
        }
    }

    pub fn new(n: usize) -> Self {
        Self {
            id: AtomicUsize::new(n),
        }
    }

    pub fn alloc(&self) -> usize {
        self.id.fetch_add(1, Ordering::AcqRel)
    }
}

impl ProcId {
    const IDLE_TASK_ID: Self = Self(0);

    fn alloc() -> Self {
        static PID_ALLOCATOR: IdAllocator = IdAllocator::zero();
        Self(PID_ALLOCATOR.alloc())
    }

    pub const fn as_usize(&self) -> usize {
        self.0
    }
}

impl TaskId {
    pub const fn as_usize(&self) -> usize {
        self.0
    }
}

impl From<usize> for ProcId {
    fn from(pid: usize) -> Self {
        Self(pid)
    }
}

impl From<usize> for TaskId {
    fn from(tid: usize) -> Self {
        Self(tid)
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

impl From<u8> for ProcState {
    fn from(state: u8) -> Self {
        match state {
            1 => Self::Normal,
            2 => Self::Stop,
            3 => Self::Zombie,
            _ => panic!("invalid proc state: {}", state),
        }
    }
}

impl Process {
    fn new_common(id: ProcId, is_kernel: bool) -> Self {
        Self {
            id,
            state: AtomicU8::new(ProcState::Normal as u8),
            is_kernel,
            exit_code: AtomicI32::new(0),
            vm: Mutex::new(None),
            tasks: Mutex::new(BTreeMap::new()),
            tid_allocator: IdAllocator::zero(),
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
            signal_actions: Mutex::new(SignalActions::default()),
        }
    }

    pub fn new_idle() -> Arc<Self> {
        let t = Arc::new(Self::new_common(ProcId::IDLE_TASK_ID, true));
        assert!(ProcId::alloc().as_usize() == 0);
        let task = Task::new_common(t.alloc_tid(), true, &t);
        task.set_state(TaskState::Running);
        t.add_task(Arc::new(task));
        t
    }

    pub fn new_kernel(entry: fn(usize) -> usize, arg: usize) -> Arc<Self> {
        let t = Arc::new(Self::new_common(ProcId::alloc(), true));
        let task = Task::new_kernel(t.alloc_tid(), &t, entry, arg);
        t.add_task(task);
        if !t.is_root() {
            ROOT_PROC.add_child(&t);
        }
        t
    }

    pub fn new_user(path: &str) -> Arc<Self> {
        let t = Arc::new(Self::new_common(ProcId::alloc(), false));

        let elf_data = open_file(path, OpenFlags::RDONLY).expect("No such user program");
        let mut vm = MemorySet::new();
        let (entry, ustack_top) = vm.load_user(elf_data.read_all().as_slice());

        let mut task = Task::new_common(t.alloc_tid(), false, &t);

        task.entry = EntryState::User(Box::new(TrapFrame::new_user(entry, ustack_top)));
        task.ctx
            .get_mut()
            .init(task_entry as _, task.kstack.top(), vm.page_table_root());

        *t.vm.lock() = Some(vm);
        t.add_task(Arc::new(task));

        ROOT_PROC.add_child(&t);
        t
    }

    pub fn user_stack(tid: usize) -> (usize, usize) {
        let top = USER_STACK_TOP - tid * (USER_STACK_SIZE + PAGE_SIZE);
        let bottom = top - USER_STACK_SIZE;
        (bottom, top)
    }

    pub fn new_user_task(self: &Arc<Self>, entry: usize, arg: usize) -> Arc<Task> {
        let tid = self.tid_allocator.alloc();
        // user stack
        let mut vm = self.vm.lock();
        let (ustack_bottom, ustack_top) = Self::user_stack(tid);
        vm.as_mut().unwrap().insert(MapArea::new_framed(
            VirtAddr::new(ustack_bottom),
            USER_STACK_SIZE,
            MemFlags::READ | MemFlags::WRITE | MemFlags::USER,
        ));
        info!(
            "{:x} = {:x?}",
            entry,
            vm.as_ref().unwrap().pt().query(VirtAddr::new(entry))
        );
        drop(vm);
        let task = Task::new_user(tid.into(), self, entry, ustack_top, arg);
        self.tasks.lock().insert(tid, task.clone());
        task
    }

    pub fn new_fork(self: &Arc<Self>, tf: &TrapFrame) -> Arc<Self> {
        assert!(!self.is_kernel());
        let t = Arc::new(Self::new_common(ProcId::alloc(), false));
        let vm = self.vm.lock().clone().unwrap();

        let mut task = Task::new_common(t.alloc_tid(), false, &t);

        task.entry = EntryState::User(Box::new(tf.new_fork()));
        task.ctx
            .get_mut()
            .init(task_entry as _, task.kstack.top(), vm.page_table_root());

        t.tasks.lock().insert(task.tid().as_usize(), Arc::new(task));

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
        self.add_child(&t);
        t
    }

    pub fn alloc_fd(&self, file: Option<Arc<dyn File + Send + Sync>>) -> usize {
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

    pub fn alloc_tid(&self) -> TaskId {
        self.tid_allocator.alloc().into()
    }

    pub fn add_task(self: &Arc<Self>, task: Arc<Task>) {
        assert!(Arc::ptr_eq(self, &task.process.upgrade().unwrap()));
        let mut tasks = self.tasks.lock();
        assert!(tasks.insert(task.tid().as_usize(), task).is_none());
    }

    pub fn add_child(self: &Arc<Self>, child: &Arc<Process>) {
        *child.parent.lock() = Arc::downgrade(self);
        self.children.lock().push(child.clone());
    }

    pub fn task_count(&self) -> usize {
        let tasks = self.tasks.lock();
        let mut count = 0;
        for task in tasks.values() {
            if task.state() != TaskState::Zombie {
                count += 1;
            }
        }
        count
    }

    pub fn task(&self) -> Arc<Task> {
        assert!(self.task_count() == 1);
        self.tasks.lock().get(&0).unwrap().clone()
    }

    pub fn first_task(&self) -> Option<Arc<Task>> {
        self.tasks
            .lock()
            .first_key_value()
            .map(|(_, value)| value.clone())
    }

    pub const fn is_root(&self) -> bool {
        self.id.as_usize() == 1
    }

    pub const fn is_idle(&self) -> bool {
        self.id.as_usize() == 0
    }

    pub fn state(&self) -> ProcState {
        self.state.load(Ordering::SeqCst).into()
    }

    pub fn set_state(&self, state: ProcState) {
        self.state.store(state as u8, Ordering::SeqCst)
    }

    pub fn pid(&self) -> ProcId {
        self.id
    }

    pub fn is_kernel(&self) -> bool {
        self.is_kernel
    }

    pub fn exit_code(&self) -> i32 {
        self.exit_code.load(Ordering::SeqCst)
    }

    pub fn set_exit_code(&self, exit_code: i32) {
        self.exit_code.store(exit_code, Ordering::SeqCst)
    }

    pub fn page_table_root(&self) -> PhysAddr {
        self.vm.lock().as_ref().unwrap().page_table_root()
    }

    #[allow(unused)]
    pub fn traverse(self: &Arc<Self>, func: &impl Fn(&Arc<Process>)) {
        func(self);
        for c in self.children.lock().iter() {
            c.traverse(func);
        }
    }
}

impl Task {
    fn new_common(tid: TaskId, is_kernel: bool, proc: &Arc<Process>) -> Self {
        Self {
            tid,
            is_kernel,
            process: Arc::downgrade(proc),

            state: AtomicU8::new(TaskState::Ready as u8),
            entry: EntryState::Kernel { pc: 0, arg: 0 },
            exit_code: AtomicI32::new(0),

            kstack: Stack::default(),
            ctx: TaskLockedCell::new(TaskContext::default()),
            signal: Mutex::new(SignalInner {
                signals: SignalFlags::empty(),
                signal_mask: SignalFlags::empty(),
                handling_sig: None,
                killed: false,
                frozen: false,
                mask_backup: None,
                trapframe_backup: None,
            }),
        }
    }

    pub fn new_kernel(
        tid: TaskId,
        proc: &Arc<Process>,
        entry: fn(usize) -> usize,
        arg: usize,
    ) -> Arc<Self> {
        let mut t = Self::new_common(tid, true, proc);
        t.entry = EntryState::Kernel {
            pc: entry as usize,
            arg,
        };
        t.ctx
            .get_mut()
            .init(task_entry as _, t.kstack.top(), PhysAddr::new(0));
        Arc::new(t)
    }

    pub fn new_user(
        tid: TaskId,
        proc: &Arc<Process>,
        entry: usize,
        ustack_top: usize,
        arg: usize,
    ) -> Arc<Self> {
        assert!(tid.as_usize() <= 16);
        let mut t = Self::new_common(tid, true, proc);
        // TODO: recycle user stack
        t.entry = EntryState::User(Box::new(TrapFrame::new_user_arg(
            entry, ustack_top, arg as _, 0,
        )));
        let token = t
            .ctx
            .get_mut()
            .init(task_entry as _, t.kstack.top(), proc.page_table_root());
        Arc::new(t)
    }

    pub fn set_singal(&self, signal: SignalFlags) {
        let mut inner = self.signal.lock();
        inner.signals |= signal;
    }

    pub fn singal_metadata(&self) -> (SignalFlags, Option<SignalFlags>) {
        let inner = self.signal.lock();
        let masked_signal = inner.signals & inner.signal_mask;
        if let Some(sig) = inner.handling_sig {
            let action = self.proc().signal_actions.lock().table[sig];
            let action_mask = action.mask;
            (masked_signal, Some(action_mask))
        } else {
            (masked_signal, None)
        }
    }

    fn kernel_signal_handler(&self, signal: SignalFlags) {
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
            // SIGKILL | SIGDEF
            _ => {
                info!(
                    "[K] call_kernel_signal_handler:: current task sigflag {:?}",
                    inner.signals
                );
                inner.killed = true;
            }
        }
    }

    fn user_signal_handler(&self, sig: usize, tf: &mut TrapFrame) {
        let action = self.proc().signal_actions.lock().table[sig];
        let mut inner = self.signal.lock();
        let handler = action.handler;
        if handler != 0 {
            // backup mask
            inner.mask_backup = Some(inner.signal_mask);
            // change current mask
            inner.signal_mask = action.mask;
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

    pub fn proc(&self) -> Arc<Process> {
        self.process.upgrade().unwrap()
    }

    pub fn is_root(&self) -> bool {
        self.proc().is_root() && self.tid.as_usize() == 0
    }

    pub fn is_idle(&self) -> bool {
        self.proc().is_idle() && self.tid.as_usize() == 0
    }

    pub fn is_kernel(&self) -> bool {
        self.is_kernel
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

    pub fn tid(&self) -> TaskId {
        self.tid
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

impl Process {
    pub fn stop(&self, exit_code: i32) {
        assert!(!self.is_idle());
        assert!(!self.is_root());

        // Make all child tasks as the children of the root task
        let mut children = self.children.lock();
        for c in children.iter() {
            ROOT_PROC.add_child(c);
        }
        children.clear();
        drop(children);

        let mut tasks = self.tasks.lock();
        for task in tasks.values() {
            if task.state() != TaskState::Zombie {
                let mut inner = task.signal.lock();
                if !inner.signals.contains(SignalFlags::SIGKILL) {
                    inner.signals.insert(SignalFlags::SIGKILL);
                }
            }
        }
        drop(tasks);
        self.set_state(ProcState::Stop);
        self.set_exit_code(exit_code);
    }

    pub fn exit(&self) {
        self.set_state(ProcState::Zombie);
        // drop memory set
        *self.vm.lock() = None;
    }

    pub fn task_exit(&self, _tid: usize, exit_code: i32) {
        if self.task_count() == 0 {
            if self.state() == ProcState::Normal {
                self.set_exit_code(exit_code);
            }
            self.exit();
        }
    }

    pub fn exec(&self, path: &str, args: Vec<String>, tf: &mut TrapFrame) -> isize {
        assert!(!self.is_kernel());
        assert!(self.task_count() == 1);
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
                if t.state() == ProcState::Zombie {
                    let child = children.remove(idx);
                    PROC_MAP.lock().remove(&child.pid().as_usize());
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

    pub fn waittid(&self, tid: usize) -> isize {
        let mut tasks = self.tasks.lock();
        if let Some(waited_task) = tasks.get(&tid).as_ref() {
            if let TaskState::Zombie = waited_task.state() {
                let code = waited_task.exit_code();
                drop(waited_task);
                tasks.remove(&tid);
                code as isize
            } else {
                -2
            }
        } else {
            // waited thread does not exist
            -1
        }
    }
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
        self.set_state(TaskState::Zombie);
        self.set_exit_code(exit_code);
        self.proc().task_exit(self.tid().as_usize(), exit_code);
        TASK_MANAGER.lock().exit_current(self, exit_code)
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
