//! ARM Generic Timer.

use cortex_a::registers::{CNTFRQ_EL0, CNTPCT_EL0, CNTP_CTL_EL0, CNTP_TVAL_EL0};
use tock_registers::interfaces::{Readable, Writeable};

use crate::arch::gicv2::irq_set_mask;
use crate::config::TICKS_PER_SEC;
use crate::sync::LazyInit;

const PHYS_TIMER_IRQ_NUM: usize = 30;

const MSEC_PER_SEC: u64 = 1000;

static CLOCK_FREQ: LazyInit<u64> = LazyInit::new();

pub fn get_time_ms() -> u64 {
    CNTPCT_EL0.get() * MSEC_PER_SEC / *CLOCK_FREQ
}

pub fn set_next_trigger() {
    CNTP_TVAL_EL0.set(*CLOCK_FREQ / TICKS_PER_SEC);
}

pub fn init() {
    CLOCK_FREQ.init_by(CNTFRQ_EL0.get());
    CNTP_CTL_EL0.write(CNTP_CTL_EL0::ENABLE::SET);
    set_next_trigger();
    irq_set_mask(PHYS_TIMER_IRQ_NUM, false);
}
