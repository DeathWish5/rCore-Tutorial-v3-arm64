pub const BOOT_KERNEL_STACK_SIZE: usize = 4096 * 4; // 16K
pub const USER_STACK_SIZE: usize = 4096 * 4; // 16K
pub const USER_STACK_TOP: usize = 0x8000_0000_0000;
pub const KERNEL_STACK_SIZE: usize = 4096 * 4; // 16K
pub const KERNEL_HEAP_SIZE: usize = 0x40_0000; // 4M

pub const USER_ASPACE_RANGE: core::ops::Range<usize> = 0..0x1_0000_0000_0000;

pub const MEMORY_START: usize = 0x4000_0000;
pub const MEMORY_END: usize = MEMORY_START + 0x800_0000;

pub const PHYS_VIRT_OFFSET: usize = 0xffff_0000_0000_0000;

pub const MAX_CPUS: usize = 1;

pub const TICKS_PER_SEC: u64 = 100;

pub use crate::board::MMIO_REGIONS;
