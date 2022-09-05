#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use cortex_a::registers::VBAR_EL1;
use tock_registers::interfaces::Writeable;

#[no_mangle]
fn main() -> i32 {
    println!("Try to access privileged CSR in U Mode");
    println!("Kernel should kill this application!");
    VBAR_EL1.set(0 as _);
    0
}
