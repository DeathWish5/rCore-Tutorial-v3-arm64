mod address;
mod frame_allocator;
mod heap_allocator;
mod memory_set;
mod page_table;
mod uaccess;

pub use address::{phys_to_virt, virt_to_phys, PhysAddr, VirtAddr};
pub use frame_allocator::{frame_alloc, frame_dealloc, PhysFrame};
pub use memory_set::{remap_test, MapArea, MemorySet};
pub use page_table::{PageTable, PageTableEntry};
pub use uaccess::{UserInOutPtr, UserInPtr, UserOutPtr};

pub const PAGE_SIZE: usize = 0x1000;

bitflags::bitflags! {
    pub struct MemFlags: usize {
        const READ          = 1 << 0;
        const WRITE         = 1 << 1;
        const EXECUTE       = 1 << 2;
        const USER          = 1 << 3;
        const DEVICE        = 1 << 4;
    }
}

pub fn init() {
    heap_allocator::init_heap();
    frame_allocator::init_frame_allocator();
    memory_set::init_paging();
}
