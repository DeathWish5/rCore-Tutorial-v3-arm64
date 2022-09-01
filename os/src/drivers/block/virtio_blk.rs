use super::BlockDevice;
use crate::mm::{frame_alloc, phys_to_virt, virt_to_phys, PhysFrame, PAGE_SIZE};
use crate::sync::Mutex;
use alloc::vec::Vec;
use lazy_static::*;
use virtio_drivers::{Hal, VirtIOBlk, VirtIOHeader};

///REF: https://github.com/qemu/qemu/blob/master/hw/arm/virt.c#L157
const VIRTIO_BASE: usize = 0x0a00_0000;

pub struct VirtIOBlock(Mutex<VirtIOBlk<'static, VirtioHal>>);

lazy_static! {
    static ref QUEUE_FRAMES: Mutex<Vec<PhysFrame>> = Mutex::new(Vec::new());
}

impl BlockDevice for VirtIOBlock {
    fn read_block(&self, block_id: usize, buf: &mut [u8]) {
        self.0
            .lock()
            .read_block(block_id, buf)
            .expect("Error when reading VirtIOBlk");
    }
    fn write_block(&self, block_id: usize, buf: &[u8]) {
        self.0
            .lock()
            .write_block(block_id, buf)
            .expect("Error when writing VirtIOBlk");
    }
}

impl VirtIOBlock {
    #[allow(unused)]
    pub fn new() -> Self {
        unsafe {
            Self(Mutex::new(
                VirtIOBlk::<VirtioHal>::new(&mut *(phys_to_virt(VIRTIO_BASE) as *mut VirtIOHeader))
                    .unwrap(),
            ))
        }
    }
}

pub struct VirtioHal;

use virtio_drivers::{PhysAddr, VirtAddr};

impl Hal for VirtioHal {
    fn dma_alloc(pages: usize) -> PhysAddr {
        let mut paddr = 0;
        for i in 0..pages {
            let frame = frame_alloc().unwrap();
            if i == 0 {
                paddr = frame.start_paddr().as_usize();
            }
            assert_eq!(frame.start_paddr().as_usize(), paddr + i * PAGE_SIZE);
            QUEUE_FRAMES.lock().push(frame);
        }
        paddr
    }

    fn dma_dealloc(_paddr: PhysAddr, _pages: usize) -> i32 {
        // TODO
        // pop from QUEUE_FRAMES
        0
    }

    fn phys_to_virt(paddr: PhysAddr) -> VirtAddr {
        phys_to_virt(paddr)
    }

    fn virt_to_phys(vaddr: VirtAddr) -> PhysAddr {
        virt_to_phys(vaddr)
    }
}
