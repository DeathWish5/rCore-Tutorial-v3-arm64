use crate::task::{spawn_task, CurrentTask};

pub fn sys_thread_create(entry: usize, arg: usize) -> isize {
    let proc = CurrentTask::get().proc();
    // create a new thread
    let new_task = proc.new_user_task(entry, arg);
    let tid = new_task.tid();
    spawn_task(new_task);
    tid.as_usize() as _
}

pub fn sys_gettid() -> isize {
    CurrentTask::get().tid().as_usize() as _
}

/// thread does not exist, return -1
/// thread has not exited yet, return -2
/// otherwise, return thread's exit code
pub fn sys_waittid(tid: usize) -> i32 {
    let task = CurrentTask::get();
    let proc = task.proc();
    // a thread cannot wait for itself
    if task.tid().as_usize() == tid {
        return -1;
    }
    proc.waittid(tid) as i32
}
