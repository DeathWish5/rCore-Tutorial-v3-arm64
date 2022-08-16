pub mod entry;
pub mod gicv2;
pub mod pl011;
pub mod psci;

pub use entry::*;
pub use gicv2::*;
pub use pl011::*;
pub use psci::*;

use core::arch::asm;

use cortex_a::registers::{DAIF, TPIDR_EL1, TTBR0_EL1, TTBR1_EL1};
use tock_registers::interfaces::{Readable, Writeable};

pub fn enable_irqs() {
    unsafe { asm!("msr daifclr, #2") };
}

pub fn disable_irqs() {
    unsafe { asm!("msr daifset, #2") };
}

pub fn irqs_disabled() -> bool {
    DAIF.matches_all(DAIF::I::Masked)
}

pub fn thread_pointer() -> usize {
    TPIDR_EL1.get() as _
}

pub unsafe fn set_thread_pointer(tp: usize) {
    TPIDR_EL1.set(tp as _)
}

pub unsafe fn activate_paging(page_table_root: usize, is_kernel: bool) {
    if is_kernel {
        // kernel space use TTBR1 (0xffff_0000_0000_0000..0xffff_ffff_ffff_ffff)
        TTBR1_EL1.set(page_table_root as _);
    } else {
        // user space use TTBR0 (0x0..0xffff_ffff_ffff)
        TTBR0_EL1.set(page_table_root as _);
    }
    flush_tlb_all();
}

pub fn flush_icache_all() {
    unsafe { asm!("ic iallu; dsb sy; isb") };
}

pub fn flush_tlb_all() {
    unsafe { asm!("tlbi vmalle1; dsb sy; isb") };
}

pub fn wait_for_ints() {
    cortex_a::asm::wfi();
}
