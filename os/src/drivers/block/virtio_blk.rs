use super::BlockDevice;
use crate::mm::{frame_alloc, phys_to_virt, virt_to_phys, PhysFrame, PAGE_SIZE};
use crate::sync::Mutex;
use alloc::vec::Vec;
use lazy_static::*;
use virtio_drivers::{VirtIOBlk, VirtIOHeader};

///REF: https://github.com/qemu/qemu/blob/master/hw/arm/virt.c#L157
const VIRTIO_BASE: usize = 0x0a00_0000;

pub struct VirtIOBlock(Mutex<VirtIOBlk<'static>>);

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
                VirtIOBlk::new(&mut *(phys_to_virt(VIRTIO_BASE) as *mut VirtIOHeader)).unwrap(),
            ))
        }
    }
}

#[no_mangle]
pub extern "C" fn virtio_dma_alloc(pages: usize) -> usize {
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

#[no_mangle]
pub extern "C" fn virtio_dma_dealloc(_pa: usize, _pages: usize) -> i32 {
    // TODO
    0
}

#[no_mangle]
pub extern "C" fn virtio_phys_to_virt(paddr: usize) -> usize {
    phys_to_virt(paddr)
}

#[no_mangle]
pub extern "C" fn virtio_virt_to_phys(vaddr: usize) -> usize {
    virt_to_phys(vaddr)
}
