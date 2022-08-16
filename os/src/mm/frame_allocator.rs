use alloc::vec::Vec;

use super::{address::virt_to_phys, PhysAddr, PAGE_SIZE};
use crate::config::MEMORY_END;
use crate::sync::SpinNoIrqLock;
use crate::utils::FreeListAllocator;

static FRAME_ALLOCATOR: SpinNoIrqLock<FreeListAllocator> =
    SpinNoIrqLock::new(FreeListAllocator::empty());

#[derive(Debug)]
pub struct PhysFrame {
    start_paddr: PhysAddr,
}

impl PhysFrame {
    pub fn alloc() -> Option<Self> {
        FRAME_ALLOCATOR.lock().alloc().map(|value| Self {
            start_paddr: PhysAddr::new(value * PAGE_SIZE),
        })
    }

    pub fn alloc_zero() -> Option<Self> {
        let mut f = Self::alloc()?;
        f.zero();
        Some(f)
    }

    pub fn start_paddr(&self) -> PhysAddr {
        self.start_paddr
    }

    pub fn zero(&mut self) {
        unsafe { core::ptr::write_bytes(self.start_paddr.into_kvaddr().as_mut_ptr(), 0, PAGE_SIZE) }
    }

    pub fn as_slice(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.start_paddr.into_kvaddr().as_ptr(), PAGE_SIZE) }
    }

    pub fn as_slice_mut(&mut self) -> &mut [u8] {
        unsafe {
            core::slice::from_raw_parts_mut(self.start_paddr.into_kvaddr().as_mut_ptr(), PAGE_SIZE)
        }
    }
}

/// allocate a frame
pub fn frame_alloc() -> Option<PhysFrame> {
    PhysFrame::alloc()
}

#[allow(dead_code)]
/// deallocate a frame
pub fn frame_dealloc(pf: PhysFrame) {
    drop(pf);
}

impl Drop for PhysFrame {
    fn drop(&mut self) {
        FRAME_ALLOCATOR
            .lock()
            .dealloc(self.start_paddr.as_usize() / PAGE_SIZE);
    }
}

pub fn init_frame_allocator() {
    extern "C" {
        fn ekernel();
    }
    let start_paddr = PhysAddr::new(virt_to_phys(ekernel as usize)).align_up();
    let end_paddr = PhysAddr::new(MEMORY_END).align_down();
    FRAME_ALLOCATOR
        .lock()
        .init(start_paddr.as_usize() / PAGE_SIZE..end_paddr.as_usize() / PAGE_SIZE);
}

#[allow(unused)]
pub fn frame_allocator_test() {
    let mut v: Vec<PhysFrame> = Vec::new();
    for i in 0..5 {
        let frame = PhysFrame::alloc().unwrap();
        println!("{:?}", frame);
        v.push(frame);
    }
    v.clear();
    for i in 0..5 {
        let frame = PhysFrame::alloc().unwrap();
        println!("{:?}", frame);
        v.push(frame);
    }
    drop(v);
    println!("frame_allocator_test passed!");
}
