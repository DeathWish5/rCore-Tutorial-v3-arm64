//! ARM Generic Timer.

use cortex_a::registers::{CNTFRQ_EL0, CNTPCT_EL0, CNTP_CTL_EL0, CNTP_TVAL_EL0};
use tock_registers::interfaces::{Readable, Writeable};

use crate::arch::gicv2::irq_set_mask;
use crate::config::TICKS_PER_SEC;
use crate::sync::LazyInit;
use crate::sync::Mutex;
use crate::task::Task;
use alloc::collections::BinaryHeap;
use alloc::sync::Arc;
use core::cmp::Ordering;

const PHYS_TIMER_IRQ_NUM: usize = 30;

const MSEC_PER_SEC: u64 = 1000;

static CLOCK_FREQ: LazyInit<u64> = LazyInit::new();
pub static TIMERS: LazyInit<Mutex<BinaryHeap<TimerCondVar>>> = LazyInit::new();

pub fn get_time_ms() -> u64 {
    CNTPCT_EL0.get() * MSEC_PER_SEC / *CLOCK_FREQ
}

pub fn set_next_trigger() {
    CNTP_TVAL_EL0.set(*CLOCK_FREQ / TICKS_PER_SEC);
}

pub fn init() {
    CLOCK_FREQ.init_by(CNTFRQ_EL0.get());
    CNTP_CTL_EL0.write(CNTP_CTL_EL0::ENABLE::SET);
    TIMERS.init_by(Mutex::new(BinaryHeap::<TimerCondVar>::new()));
    set_next_trigger();
    irq_set_mask(PHYS_TIMER_IRQ_NUM, false);
}

pub fn add_timer(expire_ms: usize, task: Arc<Task>) {
    let mut timers = TIMERS.lock();
    timers.push(TimerCondVar { expire_ms, task });
}

pub fn remove_timer(task: Arc<Task>) {
    let mut timers = TIMERS.lock();
    let mut temp = BinaryHeap::<TimerCondVar>::new();
    for condvar in timers.drain() {
        if Arc::as_ptr(&task) != Arc::as_ptr(&condvar.task) {
            temp.push(condvar);
        }
    }
    timers.clear();
    timers.append(&mut temp);
}

pub fn check_timer() {
    let current_ms = get_time_ms();
    let mut timers = TIMERS.lock();
    while let Some(timer) = timers.peek() {
        if timer.expire_ms <= current_ms as _ {
            timer.task.resume();
            timers.pop();
        } else {
            break;
        }
    }
}

pub struct TimerCondVar {
    pub expire_ms: usize,
    pub task: Arc<Task>,
}

impl PartialEq for TimerCondVar {
    fn eq(&self, other: &Self) -> bool {
        self.expire_ms == other.expire_ms
    }
}
impl Eq for TimerCondVar {}
impl PartialOrd for TimerCondVar {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let a = -(self.expire_ms as isize);
        let b = -(other.expire_ms as isize);
        Some(a.cmp(&b))
    }
}

impl Ord for TimerCondVar {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}
