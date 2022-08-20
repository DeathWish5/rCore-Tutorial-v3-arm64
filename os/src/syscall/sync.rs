use crate::sync::{Condvar, MutexBlocking, MutexSpin, Semaphore, UserMutex};
use crate::task::CurrentTask;
use crate::timer::{add_timer, get_time_ms};
use alloc::sync::Arc;

pub fn sys_sleep(ms: usize) -> isize {
    let expire_ms = get_time_ms() as usize + ms;
    let task = CurrentTask::get();
    add_timer(expire_ms, task.clone());
    task.block_and_yield();
    0
}

pub fn sys_mutex_create(blocking: bool) -> isize {
    let proc = CurrentTask::get().proc();
    let mutex: Option<Arc<dyn UserMutex>> = if !blocking {
        Some(Arc::new(MutexSpin::new()))
    } else {
        Some(Arc::new(MutexBlocking::new()))
    };
    let mut mutex_inner = proc.mutex_list.lock();
    if let Some(id) = mutex_inner
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        mutex_inner[id] = mutex;
        id as isize
    } else {
        mutex_inner.push(mutex);
        mutex_inner.len() as isize - 1
    }
}

pub fn sys_mutex_lock(mutex_id: usize) -> isize {
    let proc = CurrentTask::get().proc();
    let mutex_inner = proc.mutex_list.lock();
    let mutex = Arc::clone(mutex_inner[mutex_id].as_ref().unwrap());
    drop(mutex_inner);
    drop(proc);
    mutex.lock();
    0
}

pub fn sys_mutex_unlock(mutex_id: usize) -> isize {
    let proc = CurrentTask::get().proc();
    let mutex_inner = proc.mutex_list.lock();
    let mutex = Arc::clone(mutex_inner[mutex_id].as_ref().unwrap());
    drop(mutex_inner);
    drop(proc);
    mutex.unlock();
    0
}

pub fn sys_semaphore_create(res_count: usize) -> isize {
    let proc = CurrentTask::get().proc();
    let mut sem_inner = proc.semaphore_list.lock();
    let id = if let Some(id) = sem_inner
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        sem_inner[id] = Some(Arc::new(Semaphore::new(res_count)));
        id
    } else {
        sem_inner.push(Some(Arc::new(Semaphore::new(res_count))));
        sem_inner.len() - 1
    };
    id as isize
}

pub fn sys_semaphore_up(sem_id: usize) -> isize {
    let proc = CurrentTask::get().proc();
    let sem_inner = proc.semaphore_list.lock();
    let sem = Arc::clone(sem_inner[sem_id].as_ref().unwrap());
    drop(sem_inner);
    sem.up();
    0
}

pub fn sys_semaphore_down(sem_id: usize) -> isize {
    let proc = CurrentTask::get().proc();
    let sem_inner = proc.semaphore_list.lock();
    let sem = Arc::clone(sem_inner[sem_id].as_ref().unwrap());
    drop(sem_inner);
    sem.down();
    0
}

pub fn sys_condvar_create(_arg: usize) -> isize {
    let proc = CurrentTask::get().proc();
    let mut condv_inner = proc.condvar_list.lock();
    let id = if let Some(id) = condv_inner
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        condv_inner[id] = Some(Arc::new(Condvar::new()));
        id
    } else {
        condv_inner.push(Some(Arc::new(Condvar::new())));
        condv_inner.len() - 1
    };
    id as isize
}

pub fn sys_condvar_signal(condvar_id: usize) -> isize {
    let proc = CurrentTask::get().proc();
    let condv_inner = proc.condvar_list.lock();
    let condvar = Arc::clone(condv_inner[condvar_id].as_ref().unwrap());
    drop(condv_inner);
    condvar.signal();
    0
}

pub fn sys_condvar_wait(condvar_id: usize, mutex_id: usize) -> isize {
    let proc = CurrentTask::get().proc();
    let condv_inner = proc.condvar_list.lock();
    let mutex_inner = proc.mutex_list.lock();
    let condvar = Arc::clone(condv_inner[condvar_id].as_ref().unwrap());
    let mutex = Arc::clone(mutex_inner[mutex_id].as_ref().unwrap());
    drop(mutex_inner);
    drop(condv_inner);
    condvar.wait(mutex);
    0
}
