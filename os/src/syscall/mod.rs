const SYSCALL_READ: usize = 63;
const SYSCALL_WRITE: usize = 64;
const SYSCALL_EXIT: usize = 93;
const SYSCALL_EXIT_GROUP: usize = 94;
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
const SYSCALL_SLEEP: usize = 101;
const SYSCALL_THREAD_CREATE: usize = 1000;
const SYSCALL_GETTID: usize = 1001;
const SYSCALL_WAITTID: usize = 1002;
const SYSCALL_MUTEX_CREATE: usize = 1010;
const SYSCALL_MUTEX_LOCK: usize = 1011;
const SYSCALL_MUTEX_UNLOCK: usize = 1012;
const SYSCALL_SEMAPHORE_CREATE: usize = 1020;
const SYSCALL_SEMAPHORE_UP: usize = 1021;
const SYSCALL_SEMAPHORE_DOWN: usize = 1022;
const SYSCALL_CONDVAR_CREATE: usize = 1030;
const SYSCALL_CONDVAR_SIGNAL: usize = 1031;
const SYSCALL_CONDVAR_WAIT: usize = 1032;

mod fs;
mod process;
mod signal;
mod sync;
mod thread;

use self::fs::*;
use self::process::*;
use self::sync::*;
use self::thread::*;
use crate::trap::TrapFrame;
use signal::*;

pub fn syscall(syscall_id: usize, args: [usize; 3], tf: &mut TrapFrame) -> isize {
    // arch::enable_irqs();
    let ret = match syscall_id {
        SYSCALL_READ => sys_read(args[0], args[1].into(), args[2]),
        SYSCALL_WRITE => sys_write(args[0], args[1].into(), args[2]),
        SYSCALL_EXIT => sys_exit(args[0] as i32),
        SYSCALL_EXIT_GROUP => sys_exit_group(args[0] as i32),
        SYSCALL_YIELD => sys_yield(),
        SYSCALL_GET_TIME => sys_get_time(),
        SYSCALL_GETPID => sys_getpid(),
        SYSCALL_FORK => sys_fork(tf),
        SYSCALL_EXEC => sys_exec(args[0].into(), args[1].into(), tf),
        SYSCALL_WAITPID => sys_waitpid(args[0] as isize, args[1].into()),
        SYSCALL_OPEN => sys_open(args[0].into(), args[1] as _),
        SYSCALL_CLOSE => sys_close(args[0]),
        SYSCALL_DUP3 => sys_dup(args[0]),
        SYSCALL_PIPE2 => sys_pipe(args[0].into()),
        SYSCALL_KILL => sys_kill(args[0], args[1] as _),
        SYSCALL_SIGACTION => sys_sigaction(args[0] as _, args[1].into(), args[2].into()),
        SYSCALL_SIGPROCMASK => sys_sigprocmask(args[0] as _),
        SYSCALL_SIGRETURN => sys_sigretrun(tf),
        SYSCALL_SLEEP => sys_sleep(args[0]),
        SYSCALL_THREAD_CREATE => sys_thread_create(args[0], args[1]),
        SYSCALL_GETTID => sys_gettid(),
        SYSCALL_WAITTID => sys_waittid(args[0]) as isize,
        SYSCALL_MUTEX_CREATE => sys_mutex_create(args[0] == 1),
        SYSCALL_MUTEX_LOCK => sys_mutex_lock(args[0]),
        SYSCALL_MUTEX_UNLOCK => sys_mutex_unlock(args[0]),
        SYSCALL_SEMAPHORE_CREATE => sys_semaphore_create(args[0]),
        SYSCALL_SEMAPHORE_UP => sys_semaphore_up(args[0]),
        SYSCALL_SEMAPHORE_DOWN => sys_semaphore_down(args[0]),
        SYSCALL_CONDVAR_CREATE => sys_condvar_create(args[0]),
        SYSCALL_CONDVAR_SIGNAL => sys_condvar_signal(args[0]),
        SYSCALL_CONDVAR_WAIT => sys_condvar_wait(args[0], args[1]),
        _ => {
            println!("Unsupported syscall_id: {}", syscall_id);
            crate::task::CurrentTask::get().exit(-1);
        }
    };
    // arch::disable_irqs();
    ret
}
