## chapter2

ch2 主要完成了内核态(el1)/用户态(el0)的切换以及系统调用这一异常的处理。我们首先简单介绍 arm 异常处理的架构。

### ARM 异常处理简介

#### 异常种类

arm 异常分为两大类:

* 同步异常: 指指令执行过程中发生错误而产生的同步错误或者特殊指令，可主要分为 Aborts 与系统调用。
    * Aborts: 指在指令执行发生错误时触发，如取指错误（Instruction Abort）、数据访问异常(Data Abort)、使用非法寄存器以及除零等。地址访问异常对应 page fault 我们之后在 ch4 会处理，而其他种类的异常一般代表发生了不可恢复的错误，将用户程序杀死即可。
    * 系统调用: 之希望获取更高特权级服务而通过执行异常产生指令刻意触发的异常。有 `svc`、`hvc`、`smc`，其中 `svc` 用于用户态向内核态提出请求，也是我们该节要主要处理的内容。
* 异步中断: 指不是由于指令执行，而是外部事件产生的异步陷入。可分为 IRQ, FIQ, SError，该实验中，我们仅考虑时钟中断对应的 Irq，不处理其他类型的中断。

> 一些其他的特殊类型异常该实验不考虑。

#### 异常相关寄存器

* PSTATE (Processer state): 储存当前处理器的状态，有如下几个区域：
    * NZCV: Negative，Zero, Carry, oVerflow 状态。
    * DAIF: Debug, SError, IRQ, FIQ 屏蔽状态，我们主要关心 IRQ 的状态。
    * EL: 当前特权级。
    * SP: 栈选择位，SP=0 则所有特权级共用一个栈，SP=1 则不同特权级用不同的栈。该实验 SP=1。
    * 其他：SS, IL, nRW。该实验无需关注。
* SPSR_ELx(x=1,2,3): Saved Program Status Regiser，用来保存中断前的 PSTATE 状态。
* SP_ELx(x=0,1,2,3): 保存不同特权级栈当前位置。
* ESR_ELx(x=1,2,3): Exception Syndrome Register，保存发生中断的原因。
* ELR_ELx(x=1,2,3): Exception Link Register，中断返回地址。
* FAR_ELx(x=1,2,3)：Fault Address Register，保存访存出错的地址。
* VBAR_ELx(x=1,2,3): Vector Based Address Register，中断处理入口基址。


#### 异常处理

当异常发生时发生，硬件会自动执行一系列动作：

* 将原有 PSTATE 寄存器状态保存在 SPSR_EL1 中，保存中断原因到 ESR，保存访存到 FAR(如有必要)，保存 PC 到 ELR。
* 更新 PSTATE 寄存器（改变特权级，屏蔽中断使能，使用新特权级的栈）。
* 根据 VBAR 以及中断类型更新 PC。

![异常处理](https://documentation-service.arm.com/static/5f872814405d955c5176de2c?token=)

其中，arm 并不会直接跳转到 VBAR 开始执行，而是跳转到 VBAR + offset，offset 取决于异常类型和来源，每种异常占用 0x80 的一个向量。我们需要关注的几个 offset 见下表([完整表见这里](https://developer.arm.com/documentation/100933/0100/AArch64-exception-vector-table))。

| Address(VBAR + offset) | **Exception type** | Exception Level |
| :--------------------: | :----------------: | :-------------: |
| VBAR+0x200 (0x80 * 4)  |      同步异常      |   Current EL    |
| VBAR+0x280 (0x80 * 5)  |        IRQ         |   Current EL    |
| VBAR+0x400 (0x80 * 8)  |      同步异常      |    Lower EL     |
| VBAR+0x480 (0x80 * 9)  |        IRQ         |    Lower EL     |

#### 异常返回

在异常处理完成之后，使用 `eret` 指令可以进行异常返回，硬件会执行如下动作：

* 根据 SPSR_EL1 恢复 PSTATE 寄存器（恢复特权级，栈，中断使能）。
* 根据 ELR 恢复 PC。



### rCore-Tutorial 中的异常处理

在发生异常后，除了硬件的动作，软件还需要保存寄存器信息以供中断恢复和系统调用的处理，也就是常说的 `trapframe`:

```rust
// Trap
pub struct TrapContext {
    // General-purpose registers (X0..X30).
    pub x: [usize; 31],
    // User Stack Pointer (SP_EL0).
    pub usp: usize,
    // Exception Link Register (ELR_EL1).
    pub elr: usize,
    // Saved Process Status Register (SPSR_EL1).
    pub spsr: usize,
}
```

rCore-Tuorial-v3-arm64 采用了传统的内存模型，不考虑 KPTI，因此在用户/内核态切换的时候不需要修改页表。在 ch4 中实现进程切换的时候我们将处理进程页表。

处理异常时保存和恢复 `trapframe` 代码如下：

```assembly
.macro SAVE_REGS
    sub     sp, sp, 34 * 8
    stp     x0, x1, [sp]
    stp     x2, x3, [sp, 2 * 8]
    # ...
    stp     x26, x27, [sp, 26 * 8]
    stp     x28, x29, [sp, 28 * 8]

    mrs     x9, SP_EL0
    mrs     x10, ELR_EL1
    mrs     x11, SPSR_EL1
    stp     x30, x9, [sp, 30 * 8]
    stp     x10, x11, [sp, 32 * 8]
.endm

.macro RESTORE_REGS
    ldp     x10, x11, [sp, 32 * 8]
    ldp     x30, x9, [sp, 30 * 8]
    msr     sp_el0, x9
    msr     elr_el1, x10
    msr     spsr_el1, x11

    ldp     x28, x29, [sp, 28 * 8]
    ldp     x26, x27, [sp, 26 * 8]
    # ...
    ldp     x2, x3, [sp, 2 * 8]
    ldp     x0, x1, [sp]
    add     sp, sp, 34 * 8
.endm
```

指令解释：

* `stp`: Store Pair，可以一次存储两个值到相邻位置，比 `str` 更快。

* `mrs`: 读系统寄存器到通用寄存器。同理 `msr` 表示写通用寄存器到系统寄存器。

这里展示了保存和恢复通用寄存器以及中断相关系统寄存器的过程，我们来看看如何设置中断入口代码。我们假设已经完成如下函数

* `invalid_exception`: 处理我们不关注的异常，直接报错退出。

* `handle_sync_exception`: 处理同步异常。

* `handle_irq_exception`: 处理中断。

* `.Lexception_return`: 中断返回代码，很简单，恢复寄存器之后执行 `eret`:

  * ```
    .Lexception_return:
        RESTORE_REGS
        eret
    ```

中断入口设置：

```assembly
.section .text
.p2align 11
exception_vector_base: 
    # current EL, with SP_EL0
    INVALID_EXCP 0 0
    INVALID_EXCP 1 0
    INVALID_EXCP 2 0
    INVALID_EXCP 3 0

    # current EL, with SP_ELx
    HANDLE_SYNC
    HANDLE_IRQ
    INVALID_EXCP 2 1
    INVALID_EXCP 3 1

    # lower EL, aarch64
    HANDLE_SYNC
    HANDLE_IRQ
    INVALID_EXCP 2 2
    INVALID_EXCP 3 2

    # lower EL, aarch32
    INVALID_EXCP 0 3
    INVALID_EXCP 1 3
    INVALID_EXCP 2 3
    INVALID_EXCP 3 3
```

在介绍触发异常时的跳转时，我们忽略了很多不关注的异常，但在这里我们必须把它们补全。对于不关注的异常，我们统一用 `INVALID_EXCP` 来填写这个向量，具体定义如下：

```assembly
.macro INVALID_EXCP, kind, source
.p2align 7   # 0x80 Bytes Aligned
    SAVE_REGS
    mov     x0, sp
    mov     x1, \kind
    mov     x2, \source
    bl      invalid_exception
    b       .Lexception_return
.endm
```

可以看到，这个宏定义这段代码按照 0x80 字节对其，也就正好和异常跳转的偏移相对应。对以不关注的异常，我们调用 `invalid_exception`，三个参数分别为 trapframe 指针、中断类型、中断来源，调用完成后进行异常返回。同步异常和 IRQ 处理向量如下：

```assembly
.macro HANDLE_SYNC
.p2align 7   # 0x80 Bytes Aligned
    SAVE_REGS
    mov     x0, sp
    bl      handle_sync_exception
    b       .Lexception_return
.endm

.macro HANDLE_IRQ
.p2align 7   # 0x80 Bytes Aligned
    SAVE_REGS
    mov     x0, sp
    bl      handle_irq_exception
    b       .Lexception_return
.endm
```

这里只是传入 trapframe 指针，调用处理函数，最后异常返回。同步异常处理函数如下：

```rust
#[no_mangle]
fn handle_sync_exception(cx: &mut TrapContext) {
    let esr = ESR_EL1.extract();  // 读取中断原因
    match esr.read_as_enum(ESR_EL1::EC) {
        // 系统调用
        Some(ESR_EL1::EC::Value::SVC64) => {
            cx.x[0] = syscall(cx.x[8], [cx.x[0], cx.x[1], cx.x[2]]) as usize
        }
        // 数据访问异常
        Some(ESR_EL1::EC::Value::DataAbortLowerEL)
        | Some(ESR_EL1::EC::Value::DataAbortCurrentEL) => {
            let iss = esr.read(ESR_EL1::ISS); // 根据 ISS 区分详细原因
            println!("[kernel] Data Abort @ {:#x}, FAR = {:#x}, ISS = {:#x}", cx.elr, FAR_EL1.get(), iss);
            run_next_app(); // 目前不处理该错误，执行杀死当前程序并运行下一个
        }
        // ... Instruction Abort，同 Data Abort
        // 未知异常，直接报错
        _ => {
            panic!(
                "Unsupported synchronous exception @ {:#x}: ESR = {:#x} (EC {:#08b}, ISS {:#x})",
                cx.elr,
                esr.get(),
                esr.read(ESR_EL1::EC),
                esr.read(ESR_EL1::ISS),
            );
        }
    }
}
```



### 第一次进入用户态

返回到用户态分两种情况，其一是是处理完异常之后返回，其二第一次执行一个用户程序。事实上我们将这二者视作同一类型，区别在于第一次执行的时候，程序的状态是由我们设定的，如 ELR, PSTATE，SP 等，这些信息储存在 Trapframe 中。如下是初始化构造一个 Trapframe 的函数：

```rust
impl TrapContext {
    // entry: 用户程序入口地址，ustack_top: 用户栈栈顶
    pub fn app_init_context(entry: usize, ustack_top: usize) -> Self {
        Self {
            usp: ustack_top,
            elr: entry,
            // 特权级： EL0 
            // 打开所有中断
            spsr: (
                SPSR_EL1::M::EL0t   
                + SPSR_EL1::D::Masked
                + SPSR_EL1::A::Masked
                + SPSR_EL1::I::Masked
                + SPSR_EL1::F::Masked)
                .value as _,
            ..Default::default()
        }
    }
}
```

这里我们的用户程序是不带参数的，在 ch7 我们运行有初始参数的用户程序，那时只需要对应更新 `x0 ~ x7` 即可。

