## chapter4

ch4 我们使能了虚拟内存，这一节的主要任务是完成页表设置。本节首先介绍页表相关的一些硬件规范，然后介绍页表的设置与切换。

### VMSAv8-64 页表简介

#### 双页表

armv8 的内核态拥有两张页表，页表基址寄存器分别对应 `TTBR1_EL1` 寄存器与 `TTBR0_EL1` 寄存器。其中：

* 虚拟地址以 0xFFFF 开头，也就是 `0xFFFF_0000_0000_0000 ~ 0xFFFF_FFFF_FFFF_FFFF` 的地址使用 `TTBR1` 作为基址寄存器。这部分对应内核虚拟地址。
* 虚拟地址以 0x0000 开头，也就是 `0x0000_0000_0000_0000 ~ 0x0000_FFFF_FFFF_FFFF` 的地址使用 `TTBR0` 作为基址寄存器。这部分对应用户虚拟地址。
* 其余地址为非法地址。

![doublept](https://documentation-service.arm.com/static/5fbd26f271eff94ef49c6ff9?token=)



#### 页面大小选择

armv8 页表支持 3 中页面大小，分别为 4KB, 16KB, 64KB，可在 `TCR_EL1` (Translation Control Register) 中进行配置，该实验使用 4KB 页面大小。

#### 虚拟地址

虽然有 64 为虚拟地址，但我们已经知道高 16 位必须为全0或全1，否则地址非法。对于 4KB 大小的页面来说，其余位按照下图分配。4KB 页面使用一个 4 级页表进行翻译。

![虚拟地址](https://documentation-service.arm.com/static/5fbd26f271eff94ef49c7056?token=)

#### 页表属性介绍

###### 页基址寄存器(以 TTBR0_EL1 为例）

* bits[63:48]: ASID，进程标识符，本实验不使用。
* bits[47:12]: 顶层页表基址。
* bits[11:0]: 通常为0 (本实验不考虑这些位)。

###### 各级页表项属性

如下图，Invalid 代表无效项，Block 底层页表属性（指向一个页或者大页），Table 表示指向了下一级页表。

![](https://armv8-ref.codingbelief.com/zh/chapter_d4/figure_d4_16.png)



最低两位属性：

* bit[0]: valid，该级页表是否有效。
* bit[1]: block，是否指向下一级页表，或者指向一个大页。有 2M, 1G 两种大小的大页。
  * 为0，标志一个页面，地址位直接指向页面基址。
  * 为1，标志下一级页表，地址位指向下一级页表。
  * 顶层页表必须为1，底层必须为0，否则无效。

地址位：

* bits[12:47]: 下一级页表基址或者页面基址。

其余位属性：

![ARM](https://armv8-ref.codingbelief.com/en/chapter_d4/figure_d4_17.2.png)

完整含义可参考完整手册。其中我们需要关注的有：

* UXN: User Execute-never, 页面在 EL0 是否不可执行。

* PXN: Privileged execute-never，在 EL1 是否不可执行。
* AP[2:1]: Access Permissions，页面在各特权级读写权限。
  * AP[2] =  0，表示可读可写。AP[2] = 1，表示只读。注意一个不可读的页面没有意义。
  * AP[1] = 0，表示仅 EL1 可访问，EL0 不可访问。AP[1] = 1，EL0 也可访问。
* AF: Access Flag，是否允许加载该项进 TLB，初始通常设为 1。
* AttrIndex: Attributes Index，选择 MAIR_EL1 中定义的 8 中内存区域类型。这里我们仅需要区分 device memory (MMIO 内存)与 normal memory (普通内存)即可。 



### 初始页表配置

在使能页表后，我们需要配置一个初始页表。配置函数如下：

```rust
// 0级页表
static mut BOOT_PT_L0: [PageTableEntry; 512] = [PageTableEntry::empty(); 512];
// 1级页表
static mut BOOT_PT_L1: [PageTableEntry; 512] = [PageTableEntry::empty(); 512];

unsafe fn init_boot_page_table() {
    // 0级页表的第一项指向以下下一级页表，也就是前 512G 的映射取决于 `BOOT_PT_L1`。
    // 0x0000_0000_0000 ~ 0x0080_0000_0000, table
    BOOT_PT_L0[0] = PageTableEntry::new_table(PhysAddr::new(BOOT_PT_L1.as_ptr() as usize));、
    // 1级页表的第一项直接指向了一个 1G 的大页，且 0x0 指向 0x0，也就是第一个G为一个原地映射。属性为可读可写的 Device 内存。
    // 0x0000_0000_0000..0x0000_4000_0000, block, device memory
    BOOT_PT_L1[0] = PageTableEntry::new_page(
        PhysAddr::new(0),
        MemFlags::READ | MemFlags::WRITE | MemFlags::DEVICE,
        true,
    );
    // 1级页表的第二项直接指向了一个 1G 的大页，且 0x4000_0000 指向 0x4000_0000，则第二个G的内存也为原地映射。属性为可读可写可执行的普通内存。
    // 0x0000_4000_0000..0x0000_8000_0000, block, normal memory
    BOOT_PT_L1[1] = PageTableEntry::new_page(
        PhysAddr::new(0x4000_0000),
        MemFlags::READ | MemFlags::WRITE | MemFlags::READ | MemFlags::EXECUTE,
        true,
    );
}
```

初始页表配置了 `0x0 ~ 0x8000_0000` 为原地映射，其中 `0x0~0x4000_0000` 为 Device 内存，`0x4000_0000~0x8000_0000` 为普通内存。由于我们的内核放在 `0x4008_0000` 起始的一段内存上，而我们需要的外设也都位于 Device 内存内，这样的配置可以正常执行。

此阶段不需要考虑用户地址，故可以如下这样设置，但起作用的是 `TTBR0_EL1`。

```rust
TTBR0_EL1.set(BOOT_PT_L0.into());
TTBR1_EL1.set(root_paddr.into());
```

### 页表重映射

在完成 boot 之后，我们会重新为内核建立映射，将内核地址放在 `0xFFFF` 开头的高位地址。也就是建立一个 offset = `0xffff_0000_0000_0000` 的线性映射，这部分映射的代码可见 `mm::memoryset::init_paging()`：

```rust
pub fn init_paging() {
    let mut ms = MemorySet::new();
    let mut map_range = |start: usize, end: usize, flags: MemFlags, name: &str| {
        println!("mapping {}: [{:#x}, {:#x})", name, start, end);
        assert!(start < end);
        assert!(VirtAddr::new(start).is_aligned());
        assert!(VirtAddr::new(end).is_aligned());
        ms.insert(MapArea::new_offset(
            VirtAddr::new(start),
            PhysAddr::new(virt_to_phys(start)),
            end - start,
            flags,
        ));
    };

    // map stext section
    map_range(
        stext as usize,
        etext as usize,
        MemFlags::READ | MemFlags::EXECUTE,
        ".text",
    );
    // ..., map: rodata, data, bss, bootstack 段
    // map other memory
    map_range(
        ekernel as usize,
        phys_to_virt(MEMORY_END),
        MemFlags::READ | MemFlags::WRITE,
        "physical memory",
    );
    // map mmio memory
    for (base, size) in MMIO_REGIONS {
        map_range(
            phys_to_virt(*base),
            phys_to_virt(*base + *size),
            MemFlags::READ | MemFlags::WRITE | MemFlags::DEVICE,
            "MMIO",
        );
    }
	// activate kernel page table
    let page_table_root = ms.page_table_root();
    KERNEL_SPACE.init_by(ms);
}
```

在完成该页表后，我们设置

```rust
TTBR1_EL1.set(KERNEL_SPACE.into()); // 也就是刚刚重新建立的现行映射
TTBR0_EL1.set(0); // 非法地址 0，这保证一旦访问低地址，会立即触发错误。
```

### 用户页表与页表切换

ch4 开始使用 elf 程序作为用户程序，因此用户页表的建立与 elf 文件的解析相关联，具体代码参见`loader::load_app()`。

此时，任务的任务控制块中多了 `memory_set` 属性：

```diff
pub struct Task {
    id: usize,		
    status: TaskStatus,		
+   memory_set: Option<MemorySet>,	
    kstack: Option<Box<Stack<KERNEL_STACK_SIZE>>>,
    ctx: TaskContext,
    entry: TaskEntryStatus,
}
```

该 memory_set 关联 `TTBR0_EL1`，也就是用户部分的页表基址。

当发生任务切换时：`TTBR0_EL1` 设置为目标任务的 memory_set 对应基址，`TTBR1_EL1` 不变。若为内核任务，则设置 `TTBR0_EL1` 基址为0，也就是不能访问低地址:

```diff
fn switch_to(&self, curr_task: &Arc<Task>, next_task: Arc<Task>) {
    // 设置即将运行的进程状态
    next_task.set_state(TaskState::Running);
    // 若 next_task = curr_task，直接退出
    if Arc::ptr_eq(curr_task, &next_task) {
        return;
    }
	// 获取上下文结构体指针
    let curr_ctx_ptr = curr_task.context().as_ptr();
    let next_ctx_ptr = next_task.context().as_ptr();
	// 更新`当前进程`信息
    PerCpu::current().set_current_task(next_task);
+	// 更换用户页表 TTBR0
+    unsafe { crate::arch::activate_paging(next_ctx_ptr.ttbr0_el1 as usize, false) };
    // 调用 context_switch 函数完成切换
    unsafe { context_switch(&mut *curr_ctx_ptr, &*next_ctx_ptr) };
}
```

由于改变用户页表(`TTBR0`)不会影响内核空间映射，所以我们无需在汇编中完成这一操作。