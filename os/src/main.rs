#![no_std]
#![no_main]
#![feature(asm_sym)]
#![feature(panic_info_message)]
#![feature(alloc_error_handler)]
#![feature(naked_functions)]
#![feature(const_maybe_uninit_zeroed)]
#![feature(map_first_last)]

extern crate alloc;

#[macro_use]
extern crate log;

#[macro_use]
mod console;
mod arch;
mod board;
mod config;
mod drivers;
mod fs;
mod lang_items;
mod mm;
mod sync;
mod syscall;
mod task;
mod timer;
mod trap;
mod utils;

fn clear_bss() {
    extern "C" {
        fn sbss();
        fn ebss();
    }
    unsafe {
        core::slice::from_raw_parts_mut(sbss as usize as *mut u8, ebss as usize - sbss as usize)
            .fill(0);
    }
}

pub fn rust_main() -> ! {
    clear_bss();
    console::init();
    info!("[kernel] Hello, world!");
    trap::init();
    mm::init();
    info!("[kernel] back to world!");
    mm::remap_test();

    arch::gicv2::init();
    timer::init();
    fs::list_apps();
    task::init();
    task::run();
}
