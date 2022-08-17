use crate::fs::{make_pipe, open_file, OpenFlags};
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
        let mut count = 0;
        while count < len {
            let chunk_len = CHUNK_SIZE.min(len - count);
            let chunk: [u8; CHUNK_SIZE] = unsafe { buf.add(count).read_array(chunk_len) };
            let _len = file.write(&chunk[..chunk_len]);
            assert_eq!(_len, chunk_len);
            count += chunk_len;
        }
        count as isize
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
    let path = path.as_c_str().unwrap();
    if let Some(inode) = open_file(path, OpenFlags::from_bits(flags).unwrap()) {
        task.alloc_fd(Some(inode)) as isize
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

pub fn sys_pipe(mut pipe: UserOutPtr<usize>) -> isize {
    let task = CurrentTask::get();
    let (pipe_read, pipe_write) = make_pipe();
    let read_fd = task.alloc_fd(Some(pipe_read));
    let write_fd = task.alloc_fd(Some(pipe_write));
    pipe.write_buf(&[read_fd, write_fd]);
    0
}

pub fn sys_dup(fd: usize) -> isize {
    let task = CurrentTask::get();
    let mut fd_table = task.fd_table.lock();
    if fd >= fd_table.len() || fd_table[fd].is_none() {
        return -1;
    }
    let new_fd = if let Some(fd) = (0..fd_table.len()).find(|fd| fd_table[*fd].is_none()) {
        fd
    } else {
        fd_table.push(None);
        fd_table.len() - 1
    };
    let clone = fd_table[fd].as_ref().unwrap().clone();
    fd_table[new_fd] = Some(clone);
    new_fd as isize
}
