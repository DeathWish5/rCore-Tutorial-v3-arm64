## chapter1

ch1 主要完成 boot, 串口输出和关机的功能。该节将依次介绍这三个功能的实现。

### boot

我们观察 qemu 启动之后执行的第一段指令，可以看到：

```
0x40000000:  ldr     x0, 0x40000018  
0x40000004:  mov     x1, xzr
0x40000008:  mov     x2, xzr
0x4000000c:  mov     x3, xzr
0x40000010:  ldr     x4, 0x40000020
0x40000014:  br      x4

0x40000018:  .inst   0x44000000
0x4000001c:  .inst   0x00000000
0x40000020:  .inst   0x40080000
```

这段代码设置了一些初始寄存器的值，然后跳转到了 `x4` 指向的值，也就是 `0x40080000` 继续执行，因此 `0x40080000` 也就是我们内核的入口地址。

> x0 储存了设备树的地址，这里我们用不上，所以忽略这个参数。

参见 linker.ld:

```
ENTRY(_start)
BASE_ADDRESS = 0x40080000;

SECTIONS
{
    . = BASE_ADDRESS;
    skernel = .;

    .text : {
        stext = .;
        *(.text.entry)
        *(.text .text.*)
        . = ALIGN(4K);
        etext = .;
    }

    # ....
}
```

linker.ld 定义把 `*(.text.entry)` 段所标注的代码放在刚才所说的起始地址。注意这里的 `ENTRY` 其实标注了 elf 的入口地址，在这里是没有用的。`entry` 代码参见 entry.rs:

```rust
#[no_mangle]
#[link_section = ".text.entry"]
unsafe extern "C" fn _start() -> ! {
    asm!("
        mov     sp, {boot_stack_top}
        b       {rust_main}",
        boot_stack_top = in(reg) BOOT_STACK.as_ptr_range().end,
        rust_main = sym crate::rust_main,
        options(noreturn),
    )
}
```

这一段代码就是设立了一个静态分配的初始栈，然后跳转到 `rust_main` 函数，至此就 boot 结束了。在 `rust_main` 函数中，我们会进行一些最基本的初始化，然后输出 os 各个代码段范围，然后关机退出。接下来分别介绍串口输出和关机的实现方式。

### 串口输出

不像 riscv 一样有 sbi 的支持，在 arm 上我们需要实现简单的串口输出。由 [qemu源码](https://github.com/qemu/qemu/blob/master/hw/arm/virt.c#L146)可知，qemu 虚拟的 uart 位于 `0x900_0000` 位置，在此基础上，我们可以实现一个简易的串口驱动：

```rust
/// pl011.rs
use core::ptr::{read_volatile, write_volatile};

const UART_FIFO_DR: usize = 0x0900_0000;
const UART_FIFO_FR: usize = 0x0900_0018;

pub fn console_putchar(c: u8) {
    unsafe {
        while read_volatile(UART_FIFO_FR as *const u32) & (1 << 5) != 0 {}
        write_volatile(UART_FIFO_DR as *mut u32, c as u32);
    }
}

```

感兴趣的同学也可以实现功能更加完整强大的驱动，可参考 [qemu_plo11驱动](https://crates.io/crates/pl011_qemu)。该设备完整资料可见 [完整资料](https://documentation-service.arm.com/static/5e8e36c2fd977155116a90b5?token=)。

### 关机

arm 特权级分为四层，见下图。

![arm特权级别](https://documentation-service.arm.com/static/62dac0d5b334256d9ea8fcea?token=)

应用程序位于 el0，os 主要位于 el1，更高特权级该实验很少涉及。在 qemu boot 后我们处在 el1（这一点不同的硬件平台设定可能不一致）。与其他架构类似，当应用程序(el0) 想要调用系统调用时，可以通过 `svc` (supervisor call) 来实现，os 想要调用更高特权级指令的时候，可以通过 `hvc` (hypervisor call) 来实现。ch1 通过 `hvc` 实现关机。

```rust
/// psci.rs
const PSCI_SYSTEM_OFF: u32 = 0x8400_0008;
fn psci_hvc_call(func: u32, arg0: usize, arg1: usize, arg2: usize) -> usize {
    let ret;
    unsafe {
        asm!(
            "hvc #0",
            inlateout("x0") func as usize => ret,
            in("x1") arg0,
            in("x2") arg1,
            in("x3") arg2,
        )
    }
    ret
}

pub fn shutdown() -> ! {
    psci_hvc_call(PSCI_SYSTEM_OFF, 0, 0, 0);
    unreachable!("It should shutdown!")
}
```

