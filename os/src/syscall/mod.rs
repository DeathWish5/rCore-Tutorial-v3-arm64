const SYSCALL_READ: usize = 63;
const SYSCALL_WRITE: usize = 64;
const SYSCALL_EXIT: usize = 93;
const SYSCALL_YIELD: usize = 124;
const SYSCALL_GET_TIME: usize = 169;
const SYSCALL_GETPID: usize = 172;
const SYSCALL_FORK: usize = 220;
const SYSCALL_EXEC: usize = 221;
const SYSCALL_WAITPID: usize = 260;
const SYSCALL_OPEN: usize = 56;
const SYSCALL_CLOSE: usize = 57;
const SYSCALL_PIPE2: usize = 59;
const SYSCALL_DUP3: usize = 24;
const SYSCALL_KILL: usize = 129;
const SYSCALL_SIGACTION: usize = 134;
const SYSCALL_SIGPROCMASK: usize = 135;
const SYSCALL_SIGRETURN: usize = 139;

mod fs;
mod process;

use self::fs::*;
use self::process::*;
use crate::arch;
use crate::trap::TrapFrame;

pub fn syscall(syscall_id: usize, args: [usize; 3], tf: &mut TrapFrame) -> isize {
    // arch::enable_irqs();
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
        SYSCALL_OPEN => sys_open(args[0].into(), args[1] as _),
        SYSCALL_CLOSE => sys_close(args[0]),
        SYSCALL_DUP3 => sys_dup(args[0]),
        SYSCALL_PIPE2 => sys_pipe(args[0].into()),
        SYSCALL_KILL => sys_kill(args[0], args[1] as _),
        SYSCALL_SIGACTION => sys_sigaction(args[0] as _, args[1].into(), args[2].into()),
        SYSCALL_SIGPROCMASK => sys_sigprocmask(args[0] as _),
        SYSCALL_SIGRETURN => sys_sigretrun(tf),
        _ => {
            println!("Unsupported syscall_id: {}", syscall_id);
            crate::task::CurrentTask::get().exit(-1);
        }
    };
    // arch::disable_irqs();
    ret
}
