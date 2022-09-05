#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

static TESTS: &[&str] = &[
    "exit\0",
    "fantastic_text\0",
    "forktest\0",
    "forktest2\0",
    "forktest_simple\0",
    "hello_world\0",
    "matrix\0",
    "sleep\0",
    "sleep_simple\0",
    "stack_overflow\0",
    "yield\0",
    "cat_filea\0",
    "huge_write\0",
    "pipe_large_test\0",
    "pipetest\0",
    "run_pipe_test\0",
    "sig_simple\0",
    "sig_simple2\0",
    "sig_tests\0",
    "run_cmdline_args\0",
    "threads_arg\0",
    "threads\0",
    "mpsc_sem\0",
    "stackless_coroutine\0",
    "stackful_coroutine\0",
    "sync_sem\0",
    "test_condvar\0",
    "race_adder_arg\0",
    "race_adder_atomic\0",
    "race_adder_mutex_blocking\0",
    "race_adder_mutex_spin\0",
    // fail tests
    "priv_csr\0",
    "priv_inst\0",
    "store_fault\0",
    "stack_overflow\0",
    "race_adder\0",
    "race_adder_loop\0",
];

use user_lib::{exec, fork, waitpid};

#[no_mangle]
pub fn main() -> i32 {
    for test in TESTS {
        println!("Usertests: Running '{}':", test);
        let pid = fork();
        if pid == 0 {
            if exec(*test, &[core::ptr::null::<u8>()]) == -1 {
                panic!("usertest '{}' not found!", test);
            } else {
                panic!("unreachable!");
            }
        } else {
            let mut exit_code: i32 = 0;
            let wait_pid = waitpid(pid as usize, &mut exit_code);
            assert_eq!(pid, wait_pid);
            let color = if exit_code == 0 { 32 } else { 31 };
            println!(
                "\x1b[{}mUsertests: Test '{}' in Process {} exited with code {}.\x1b[0m",
                color, test, pid, exit_code
            );
        }
    }
    println!("usertests passed!");
    0
}
