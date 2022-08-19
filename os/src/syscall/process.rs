use crate::mm::{UserInOutPtr, UserInPtr, UserOutPtr};
use crate::task::{
    pid2proc, spawn_proc, spawn_task, CurrentTask, SignalAction, SignalFlags, MAX_SIG,
};
use crate::timer::get_time_ms;
use crate::trap::TrapFrame;
use alloc::{string::String, vec::Vec};

const MAX_STR_LEN: usize = 256;

pub fn sys_exit(exit_code: i32) -> ! {
    CurrentTask::get().exit(exit_code);
}

pub fn sys_yield() -> isize {
    CurrentTask::get().yield_now();
    0
}

pub fn sys_get_time() -> isize {
    get_time_ms() as isize
}

pub fn sys_getpid() -> isize {
    CurrentTask::get().proc().pid().as_usize() as isize
}

pub fn sys_fork(tf: &TrapFrame) -> isize {
    let new_proc = CurrentTask::get().proc().new_fork(tf);
    let pid = new_proc.pid().as_usize() as isize;
    spawn_proc(new_proc.clone());
    spawn_task(new_proc.task());
    pid
}

fn read_argvs(args: UserInPtr<*const u8>) -> Vec<String> {
    let mut args_vec = Vec::<String>::new();
    let mut argc = 0;
    loop {
        let arg = unsafe { args.add(argc).read() } as usize;
        if arg as usize == 0 {
            break;
        }
        let arg: UserInPtr<u8> = UserInPtr::from(arg);
        let s = arg.read_c_str().expect("Invalid args");
        args_vec.push(s);
        argc += 1;
    }
    args_vec
}

pub fn sys_exec(path: UserInPtr<u8>, args: UserInPtr<*const u8>, tf: &mut TrapFrame) -> isize {
    let (path_buf, len) = path.read_str::<MAX_STR_LEN>();
    let path = core::str::from_utf8(&path_buf[..len]).unwrap();
    CurrentTask::get().proc().exec(path, read_argvs(args), tf)
}

/// If there is no child process has the same pid as the given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, mut exit_code_ptr: UserOutPtr<i32>) -> isize {
    let mut exit_code = 0;
    let ret = CurrentTask::get().proc().waitpid(pid, &mut exit_code);
    exit_code_ptr.write(exit_code);
    ret
}

pub fn sys_kill(pid: usize, signum: i32) -> isize {
    if let Some(proc) = pid2proc(pid) {
        if let Some(flag) = SignalFlags::from_bits(1 << signum) {
            // insert the signal if legal
            let mut inner = proc.signal.lock();
            if inner.signals.contains(flag) {
                return -1;
            }
            inner.signals.insert(flag);
            0
        } else {
            -1
        }
    } else {
        -1
    }
}

pub fn sys_sigprocmask(mask: u32) -> isize {
    let proc = CurrentTask::get().proc();
    let mut inner = proc.signal.lock();
    let old_mask = inner.signal_mask;
    if let Some(flag) = SignalFlags::from_bits(mask) {
        inner.signal_mask = flag;
        old_mask.bits() as isize
    } else {
        -1
    }
}

pub fn sys_sigretrun(tf: &mut TrapFrame) -> isize {
    let proc = CurrentTask::get().proc();
    let mut inner = proc.signal.lock();
    if let Some(backup) = inner.trapframe_backup.take() {
        inner.handling_sig = None;
        inner.signal_mask = inner.mask_backup.take().unwrap();
        *tf = backup;
        return 0;
    }
    -1
}

pub fn sys_sigaction(
    signum: i32,
    action: UserInPtr<SignalAction>,
    mut old_action: UserInOutPtr<SignalAction>,
) -> isize {
    if signum as usize > MAX_SIG {
        return -1;
    }
    if let Some(signal) = SignalFlags::from_bits(1 << signum) {
        if action.is_null()
            || old_action.is_null()
            || signal == SignalFlags::SIGKILL
            || signal == SignalFlags::SIGSTOP
        {
            return -1;
        }
        let proc = CurrentTask::get().proc();
        let mut inner = proc.signal.lock();
        let old_kernel_action = inner.signal_actions.table[signum as usize];
        if old_kernel_action.mask != SignalFlags::TRAP_QUIT {
            old_action.write(old_kernel_action);
        } else {
            let mut action = old_action.read();
            action.handler = old_kernel_action.handler;
            old_action.write(action);
        }
        inner.signal_actions.table[signum as usize] = action.read();
        return 0;
    }
    -1
}
