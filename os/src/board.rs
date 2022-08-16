pub const MMIO_REGIONS: &[(usize, usize)] = &[
    (0x0a00_0000, 0x1000),   // VIRT MMIO
    (0x0900_0000, 0x1000),   // PL011 UART
    (0x0800_0000, 0x2_0000), // GICv2
];

pub type BlockDeviceImpl = crate::drivers::block::VirtIOBlock;
