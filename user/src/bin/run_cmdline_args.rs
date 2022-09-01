#![no_std]
#![no_main]

extern crate user_lib;

use user_lib::exec;

const ARGS: [*const u8; 5] = [
    "1\0".as_ptr(),
    "555~\0".as_ptr(),
    "QAQ\0".as_ptr(),
    "(*o*)\0".as_ptr(),
    core::ptr::null::<u8>(),
];

#[no_mangle]
pub fn main() -> i32 {
    exec("cmdline_args\0", &ARGS);
    0
}
