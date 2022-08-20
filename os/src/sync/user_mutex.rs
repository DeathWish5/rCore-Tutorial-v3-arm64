use super::Mutex;
use crate::task::{CurrentTask, Task};
use alloc::{collections::VecDeque, sync::Arc};

pub trait UserMutex: Sync + Send {
    fn lock(&self);
    fn unlock(&self);
}

pub struct MutexSpin {
    locked: Mutex<bool>,
}

impl MutexSpin {
    pub fn new() -> Self {
        Self {
            locked: Mutex::new(false),
        }
    }
}

impl UserMutex for MutexSpin {
    fn lock(&self) {
        loop {
            let mut locked = self.locked.lock();
            if *locked {
                drop(locked);
                CurrentTask::get().yield_now();
                continue;
            } else {
                *locked = true;
                return;
            }
        }
    }

    fn unlock(&self) {
        let mut locked = self.locked.lock();
        *locked = false;
    }
}

pub struct MutexBlocking {
    inner: Mutex<MutexBlockingInner>,
}

pub struct MutexBlockingInner {
    locked: bool,
    wait_queue: VecDeque<Arc<Task>>,
}

impl MutexBlocking {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(MutexBlockingInner {
                locked: false,
                wait_queue: VecDeque::new(),
            }),
        }
    }
}

impl UserMutex for MutexBlocking {
    fn lock(&self) {
        let mut mutex_inner = self.inner.lock();
        if mutex_inner.locked {
            let curr_task = CurrentTask::get();
            mutex_inner.wait_queue.push_back(curr_task.clone());
            drop(mutex_inner);
            curr_task.block_and_yield();
        } else {
            mutex_inner.locked = true;
        }
    }

    fn unlock(&self) {
        let mut mutex_inner = self.inner.lock();
        assert!(mutex_inner.locked);
        if let Some(waking_task) = mutex_inner.wait_queue.pop_front() {
            waking_task.resume();
        } else {
            mutex_inner.locked = false;
        }
    }
}
