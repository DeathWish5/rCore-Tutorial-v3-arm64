use crate::sync::{Mutex, UserMutex};
use crate::task::{CurrentTask, Task};
use alloc::{collections::VecDeque, sync::Arc};

pub struct Condvar {
    pub inner: Mutex<CondvarInner>,
}

pub struct CondvarInner {
    pub wait_queue: VecDeque<Arc<Task>>,
}

impl Condvar {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(CondvarInner {
                wait_queue: VecDeque::new(),
            }),
        }
    }

    pub fn signal(&self) {
        let mut inner = self.inner.lock();
        if let Some(task) = inner.wait_queue.pop_front() {
            task.resume();
        }
    }

    pub fn wait(&self, mutex: Arc<dyn UserMutex>) {
        mutex.unlock();
        let mut inner = self.inner.lock();
        let curr_task = CurrentTask::get();
        inner.wait_queue.push_back(curr_task.clone());
        drop(inner);
        curr_task.block_and_yield();
        mutex.lock();
    }
}
