use super::Mutex;
use crate::task::{CurrentTask, Task};
use alloc::{collections::VecDeque, sync::Arc};

pub struct Semaphore {
    pub inner: Mutex<SemaphoreInner>,
}

pub struct SemaphoreInner {
    pub count: isize,
    pub wait_queue: VecDeque<Arc<Task>>,
}

impl Semaphore {
    pub fn new(res_count: usize) -> Self {
        Self {
            inner: Mutex::new(SemaphoreInner {
                count: res_count as isize,
                wait_queue: VecDeque::new(),
            }),
        }
    }

    pub fn up(&self) {
        let mut inner = self.inner.lock();
        inner.count += 1;
        if inner.count <= 0 {
            if let Some(task) = inner.wait_queue.pop_front() {
                task.resume();
            }
        }
    }

    pub fn down(&self) {
        let mut inner = self.inner.lock();
        inner.count -= 1;
        if inner.count < 0 {
            let task = CurrentTask::get();
            inner.wait_queue.push_back(task.clone());
            drop(inner);
            task.block_and_yield();
        }
    }
}
