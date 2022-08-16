use crate::fs::{open_file, OpenFlags};
use crate::mm::{UserInPtr, UserOutPtr};
use crate::task::CurrentTask;

const CHUNK_SIZE: usize = 256;

pub fn sys_write(fd: usize, buf: UserInPtr<u8>, len: usize) -> isize {
    let task = CurrentTask::get();
    let fd_table = task.fd_table.lock();
    if fd >= fd_table.len() {
        return -1;
    }
    if let Some(file) = &fd_table[fd] {
        if !file.writable() {
            return -1;
        }
        let file = file.clone();
        // release current task TCB manually to avoid multi-borrow
        drop(fd_table);
        file.write(&buf.read_array::<CHUNK_SIZE>(len)[..len]) as isize
    } else {
        -1
    }
}

pub fn sys_read(fd: usize, mut buf: UserOutPtr<u8>, len: usize) -> isize {
    let task = CurrentTask::get();
    let fd_table = task.fd_table.lock();
    if fd >= fd_table.len() {
        return -1;
    }
    if let Some(file) = &fd_table[fd] {
        if !file.readable() {
            return -1;
        }
        let file = file.clone();
        // release current task TCB manually to avoid multi-borrow
        drop(fd_table);
        let mut buffer = alloc::vec![0u8; len];
        let len = file.read(&mut buffer) as isize;
        buf.write_buf(&buffer);
        len
    } else {
        -1
    }
}

pub fn sys_open(path: UserInPtr<u8>, flags: u32) -> isize {
    let task = CurrentTask::get();
    if let Some(inode) = open_file(
        path.as_c_str().unwrap(),
        OpenFlags::from_bits(flags).unwrap(),
    ) {
        let fd = task.alloc_fd();
        let mut fd_table = task.fd_table.lock();
        fd_table[fd] = Some(inode);
        fd as isize
    } else {
        -1
    }
}

pub fn sys_close(fd: usize) -> isize {
    let task = CurrentTask::get();
    let mut fd_table = task.fd_table.lock();
    if fd >= fd_table.len() {
        return -1;
    }
    if fd_table[fd].is_none() {
        return -1;
    }
    fd_table[fd].take();
    0
}
