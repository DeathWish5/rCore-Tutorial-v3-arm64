## chapter3

ch3 我们完成了任务切换和时钟中断管理。

### 任务上下文与任务切换

为了完成任务切换，我们需要实现一段保存和恢复别调用着保存寄存器的汇编。由于我们选用的内存模型，我们理论上还需要更换进程页表，但 ch3 还没有使能页表，所以我们留到 ch4 再处理。

类似与其他架构，任务切换需要保存和恢复

* sp: 栈指针，这里对应 SP_EL1。
* ra: 返回地址，在 arm64 中为 x30 寄存器。
* 被调用这保存寄存器，在 arm64 中为 x19~x29 寄存器。
* TPIDR寄存器，Thread Process ID Registers，也就是线程局部存储寄存器，该寄存器也属于线程私有资源，需要保存和恢复。不过本实验由于用户态比较简单，其实并不会用到。

任务上下文结构体定义如下：

```rust
#[repr(C)]
#[derive(Debug)]
pub struct TaskContext {
    pub sp: usize,			// SP_EL1
    pub tpidr_el0: usize,	// Thread Process ID Registers
    pub r: [usize; 10],		// r19~r29
    pub lr: usize, 			// r30
}
```

我们使用一个内联函数完成任务上下文切换，比较简单的保存和恢复。

```rust
pub unsafe fn context_switch(current_task: &mut TaskContext, next_task: &TaskContext) {
    asm!("
        // save callee-saved registers, x19~x30
        stp     x29, x30, [x0, 12 * 8]
        stp     x27, x28, [x0, 10 * 8]
        ...
        stp     x21, x22, [x0, 4 * 8]
        stp     x19, x20, [x0, 2 * 8]
        // save sp and TPIDR
        mov     x19, sp
        mrs     x20, tpidr_el0
        stp     x19, x20, [x0]

        // restore sp and TPIDR
        ldp     x19, x20, [x1]
        mov     sp, x19
        msr     tpidr_el0, x20
        // restore callee-saved registers, x19~x30
        ldp     x19, x20, [x1, 2 * 8]
        ldp     x21, x22, [x1, 4 * 8]
        ...
        ldp     x27, x28, [x1, 10 * 8]
        ldp     x29, x30, [x1, 12 * 8]

        ret",
        in("x0") current_task,
        in("x1") next_task,
        options(noreturn),
    )
}

```

### 时钟中断处理

时钟相关寄存器:

* CNTP_CTL: Counter-timer Physical Control Register，控制寄存器，控制时钟中断使能。

* CNTFRQ: Counter-timer Frequency Register，时钟频率寄存器。
* CNTPCT: Counter-timer Physical Count Register，当前时钟计数。
* CNTP_CVAL: Counter-timer Physical Timer Compare Value Register。CNTPCT 达到何值时，产生一个时钟中断。
* CNTP_TVAL: Counter-timer Physical Timer Value Register。与 CNTP_CVAL 相关联的特殊寄存器，当写入一个值的时候，CNTP_CVAL 自动设定为 CNTPCT + CNTP_TVAL，可以方便的更新 CNTP_CVAL。

时钟中断相关函数：

```rust
/// timer.rs
static CLOCK_FREQ: LazyInit<u64> = LazyInit::new();

// 当前时间(毫秒)= 当前计数(CNTPCT) * 1000(秒转毫秒) / 频率 (CLOCK_FREQ) 
pub fn get_time_ms() -> u64 {
    CNTPCT_EL0.get() * 1000 / *CLOCK_FREQ
}

// 设置下一次时钟中断时间，写 CNTP_TVAL = 频率 (CLOCK_FREQ) / 每秒希望的中断数量
pub fn set_next_trigger() {
    CNTP_TVAL_EL0.set(*CLOCK_FREQ / TICKS_PER_SEC);
}

// 初始化时钟中断
pub fn init() {
    CLOCK_FREQ.init_by(CNTFRQ_EL0.get()); 			// 读取频率
    CNTP_CTL_EL0.write(CNTP_CTL_EL0::ENABLE::SET);	// 使能时钟
    set_next_trigger();								// 设置第一次中断
    irq_set_mask(PHYS_TIMER_IRQ_NUM, false);		// 打开全局中断中的时钟中断
}
```

时钟中断的全局使能和处理需要了解 GIC (Generic Interrupt Controller) 相关的内容，感兴趣的同学可以看[GIC](https://developer.arm.com/documentation/ihi0069/latest) 。这里我们只需要知道，读写对应的内存地址可以

1. 使能/禁止不同中断。

2. 读取 pending 的中断信息，如中断类型。

3. 清除pending的中断，也就是告知中断处理完毕。

利用 GIC 的功能1，实现了 `irq_set_mask` 也就是开启全局中断。利用功能2、3，以及 ch2 中提到的中断异常处理代码，我们实现了时钟中断的处理。

时钟中断属于一种 IRQ，所以中断入口在 ch2 提到的 `HANDLE_IRQ` 向量，该向量会进一步调用 `handle_irq_exception` 函数，我们来看看该函数的实现：

```rust
#[no_mangle]
fn handle_irq_exception(_cx: &mut TrapFrame) {
    match crate::gicv2::handle_irq() {
        // 发生时钟中断，切换任务
        IrqHandlerResult::Reschedule => CurrentTask::yield_now(),
        // 其他中断暂时不处理
        _ => {},
    }
}
```

来看看 `gicv2::handle_irq` 函数：

```rust
pub fn handle_irq() -> IrqHandlerResult {
    // 读取中断编号
    if let Some(vector) = GIC.pending_irq() {
        let res = match vector {
            // 如果是时钟中断
            30 => {
                // 设置下一次时钟中断
                crate::timer::set_next_trigger();
                // 告知外部需要切换任务
                IrqHandlerResult::Reschedule
            }
            // 其他中断暂不处理
            _ => IrqHandlerResult::NoReschedule,
        };
        // 告知硬件对应中断处理完毕
        GIC.eoi(vector);
        res
    } else {
        IrqHandlerResult::NoReschedule
    }
}
```

> gicv2 这个名字代表该设备采用了 GIC 标准 v2.0。该设备地址地址见 [qemu源码](https://github.com/qemu/qemu/blob/master/hw/arm/virt.c#L137)。

