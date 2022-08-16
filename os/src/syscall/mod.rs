const SYSCALL_READ: usize = 63;
const SYSCALL_WRITE: usize = 64;
const SYSCALL_EXIT: usize = 93;
const SYSCALL_YIELD: usize = 124;
const SYSCALL_GET_TIME: usize = 169;
const SYSCALL_GETPID: usize = 172;
const SYSCALL_FORK: usize = 220;
const SYSCALL_EXEC: usize = 221;
const SYSCALL_WAITPID: usize = 260;
const SYSCALL_OPEN: usize = 56; // TODO
const SYSCALL_CLOSE: usize = 57; // TODO

mod fs;
mod process;

use self::fs::*;
use self::process::*;
use crate::arch;
use crate::trap::TrapFrame;

pub fn syscall(syscall_id: usize, args: [usize; 3], tf: &mut TrapFrame) -> isize {
    arch::enable_irqs();
    let ret = match syscall_id {
        SYSCALL_READ => sys_read(args[0], args[1].into(), args[2]),
        SYSCALL_WRITE => sys_write(args[0], args[1].into(), args[2]),
        SYSCALL_EXIT => sys_exit(args[0] as i32),
        SYSCALL_YIELD => sys_yield(),
        SYSCALL_GET_TIME => sys_get_time(),
        SYSCALL_GETPID => sys_getpid(),
        SYSCALL_FORK => sys_fork(tf),
        SYSCALL_EXEC => sys_exec(args[0].into(), tf),
        SYSCALL_WAITPID => sys_waitpid(args[0] as isize, args[1].into()),
        SYSCALL_OPEN => sys_open(args[0].into(), args[1] as u32),
        SYSCALL_CLOSE => sys_close(args[0]),
        _ => {
            println!("Unsupported syscall_id: {}", syscall_id);
            crate::task::CurrentTask::get().exit(-1);
        }
    };
    arch::disable_irqs();
    ret
}
