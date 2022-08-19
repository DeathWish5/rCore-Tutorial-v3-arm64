mod manager;
mod percpu;
mod schedule;
mod signal;
mod structs;
mod switch;

use alloc::sync::Arc;

pub use manager::pid2proc;
pub use signal::*;
pub use structs::{CurrentTask, ProcId};

use self::manager::{PROC_MAP, TASK_MANAGER};
use self::structs::{Process, Task, ROOT_PROC};

pub fn init() {
    percpu::init_percpu();
    manager::init();

    ROOT_PROC.init_by(Process::new_kernel(
        |_| loop {
            let curr_task = CurrentTask::get();
            let curr_proc = curr_task.proc();
            let mut exit_code = 0;
            while curr_proc.waitpid(-1, &mut exit_code) > 0 {}
            if curr_proc.children.lock().len() == 0 {
                crate::arch::wait_for_ints();
            } else {
                curr_task.yield_now();
            }
        },
        0,
    ));

    let test_kernel_task = |arg| {
        println!(
            "test kernel task: pid = {:?}, arg = {:#x}",
            CurrentTask::get().proc().pid(),
            arg
        );
        0
    };

    let mut m = TASK_MANAGER.lock();
    let root_task = ROOT_PROC.task();
    assert!(root_task.is_root());
    m.spawn(root_task);
    m.spawn(Process::new_kernel(test_kernel_task, 0xdead).task());
    m.spawn(Process::new_kernel(test_kernel_task, 0xbeef).task());
    m.spawn(Process::new_user("user_shell").task());
}

pub fn spawn_proc(proc: Arc<Process>) {
    PROC_MAP.lock().insert(proc.pid().as_usize(), proc);
}

pub fn spawn_task(task: Arc<Task>) {
    TASK_MANAGER.lock().spawn(task);
}

pub fn run() -> ! {
    crate::arch::enable_irqs();
    CurrentTask::get().yield_now(); // current task is idle at this time
    unreachable!("root task exit!");
}
