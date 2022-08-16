use alloc::sync::{Arc, Weak};
use alloc::{boxed::Box, vec::Vec};
use core::sync::atomic::{AtomicI32, AtomicU8, AtomicUsize, Ordering};

use super::manager::{TaskLockedCell, TASK_MANAGER};
use super::percpu::PerCpu;
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

    pub fn exec(&self, path: &str, tf: &mut TrapFrame) -> isize {
        assert!(!self.is_kernel_task());
        if let Some(elf_data) = open_file(path, OpenFlags::RDONLY) {
            let mut vm = self.vm.lock();
            let vm = vm.get_or_insert(MemorySet::new());
            vm.clear();
            let (entry, ustack_top) = vm.load_user(elf_data.read_all().as_slice());
            *tf = TrapFrame::new_user(entry, ustack_top);
            crate::arch::flush_tlb_all();
            0
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

    pub fn alloc_fd(&self) -> usize {
        let mut fd_table = self.fd_table.lock();
        if let Some(fd) = (0..fd_table.len()).find(|fd| fd_table[*fd].is_none()) {
            fd
        } else {
            fd_table.push(None);
            fd_table.len() - 1
        }
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
