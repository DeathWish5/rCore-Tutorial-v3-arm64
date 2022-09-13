use crate::mm::{UserInOutPtr, UserInPtr};
use crate::task::{pid2proc, CurrentTask, SignalAction, SignalFlags, TaskState, MAX_SIG};
use crate::trap::TrapFrame;

pub fn sys_kill(pid: usize, signum: i32) -> isize {
    if let Some(proc) = pid2proc(pid) {
        if let Some(flag) = SignalFlags::from_bits(1 << signum) {
            // insert the signal if legal
            // signal mask??
            let tasks = proc.tasks.lock();
            let mut count = 0;
            for task in tasks.values().cloned() {
                if task.state() != TaskState::Zombie {
                    let mut inner = task.signal.lock();
                    if !inner.signals.contains(flag) {
                        inner.signals.insert(flag);
                        count += 1;
                    }
                }
            }
            if count > 0 {
                return 0;
            }
            // FIX ME: bug from rCore-tutorial-v3
            return 0;
        }
    }
    -1
}

pub fn sys_sigprocmask(mask: u32) -> isize {
    let task = CurrentTask::get();
    let mut inner = task.signal.lock();
    let old_mask = inner.signal_mask;
    if let Some(flag) = SignalFlags::from_bits(mask) {
        inner.signal_mask = flag;
        old_mask.bits() as isize
    } else {
        -1
    }
}

pub fn sys_sigretrun(tf: &mut TrapFrame) -> isize {
    let task = CurrentTask::get();
    let mut inner = task.signal.lock();
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
        let mut actions = proc.signal_actions.lock();
        let old_kernel_action = actions.table[signum as usize];
        if old_kernel_action.mask != SignalFlags::TRAP_QUIT {
            old_action.write(old_kernel_action);
        } else {
            let mut action = old_action.read();
            action.handler = old_kernel_action.handler;
            old_action.write(action);
        }
        actions.table[signum as usize] = action.read();
        return 0;
    }
    -1
}
