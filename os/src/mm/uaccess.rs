#![allow(dead_code)]
#![allow(clippy::uninit_assumed_init)]

use crate::config::USER_ASPACE_RANGE;
use alloc::string::String;
use core::marker::PhantomData;
use core::mem::{align_of, size_of, MaybeUninit};
use core::result::Result;

const fn uaccess_ok(vaddr: usize, size: usize) -> bool {
    vaddr != 0 && USER_ASPACE_RANGE.start <= vaddr && vaddr <= USER_ASPACE_RANGE.end - size
}

unsafe fn copy_from_user<T>(kdst: *mut T, usrc: *const T, len: usize) {
    assert!(uaccess_ok(usrc as usize, len * size_of::<T>()));
    kdst.copy_from_nonoverlapping(usrc, len);
}

unsafe fn copy_to_user<T>(udst: *mut T, ksrc: *const T, len: usize) {
    assert!(uaccess_ok(udst as usize, len * size_of::<T>()));
    udst.copy_from_nonoverlapping(ksrc, len);
}

unsafe fn copy_from_user_str(kdst: *mut u8, usrc: *const u8, max_len: usize) -> usize {
    assert!(uaccess_ok(usrc as usize, 1));
    let mut len = 0;
    let mut kdst = kdst;
    let mut usrc = usrc;
    while len < max_len {
        assert!((usrc as usize) < USER_ASPACE_RANGE.end);
        let c = usrc.read();
        if c == b'\0' {
            break;
        }
        kdst.write(c);
        len += 1;
        kdst = kdst.add(1);
        usrc = usrc.add(1);
    }
    kdst.write(b'\0');
    len
}

pub trait Policy {}
pub trait ReadPolicy: Policy {}
pub trait WritePolicy: Policy {}
pub enum In {}
pub enum Out {}
pub enum InOut {}

impl Policy for In {}
impl ReadPolicy for In {}
impl Policy for Out {}
impl WritePolicy for Out {}
impl Policy for InOut {}
impl ReadPolicy for InOut {}
impl WritePolicy for InOut {}

pub type UserInPtr<T> = UserPtr<T, In>;
pub type UserOutPtr<T> = UserPtr<T, Out>;
pub type UserInOutPtr<T> = UserPtr<T, InOut>;

pub struct UserPtr<T, P: Policy> {
    ptr: *mut T,
    _phantom: PhantomData<P>,
}

impl<T, P: Policy> From<usize> for UserPtr<T, P> {
    fn from(user_vaddr: usize) -> Self {
        Self {
            ptr: user_vaddr as *mut T,
            _phantom: PhantomData,
        }
    }
}

impl<T, P: Policy> UserPtr<T, P> {
    pub fn is_null(&self) -> bool {
        self.ptr as usize == 0
    }

    pub fn check(&self) -> bool {
        let vaddr = self.ptr as usize;
        uaccess_ok(vaddr, 1) && (vaddr % align_of::<T>() == 0)
    }

    pub fn as_ptr(&self) -> *const T {
        self.ptr
    }

    pub fn as_mut_ptr(&mut self) -> *mut T {
        self.ptr
    }

    pub fn as_slice(&self, len: usize) -> Result<&'static [T], &'static str> {
        if len == 0 {
            Ok(&[])
        } else {
            assert!(uaccess_ok(self.ptr as usize, len));
            Ok(unsafe { core::slice::from_raw_parts(self.ptr, len) })
        }
    }

    pub unsafe fn add(&self, count: usize) -> Self {
        Self {
            ptr: self.ptr.add(count),
            _phantom: PhantomData,
        }
    }
}

impl<T, P: ReadPolicy> UserPtr<T, P> {
    pub fn read(&self) -> T {
        assert!(self.check());
        let mut value = MaybeUninit::uninit();
        unsafe {
            copy_from_user(value.as_mut_ptr(), self.ptr, 1);
            value.assume_init()
        }
    }

    pub fn read_array<const N: usize>(&self, max_len: usize) -> [T; N] {
        assert!(self.check());
        let mut buf: [T; N] = unsafe { MaybeUninit::uninit().assume_init() };
        unsafe { copy_from_user(buf.as_mut_ptr(), self.ptr, max_len.min(N)) };
        buf
    }
}

const C_STR_MAX_LEN: usize = 256;

impl<P: ReadPolicy> UserPtr<u8, P> {
    pub fn read_str<const N: usize>(&self) -> ([u8; N], usize) {
        assert!(self.check());
        let mut buf: [u8; N] = unsafe { MaybeUninit::uninit().assume_init() };
        let len = unsafe { copy_from_user_str(buf.as_mut_ptr(), self.ptr, N - 1) };
        (buf, len)
    }

    pub fn read_c_str(&self) -> Result<String, &'static str> {
        self.as_c_str().map(|s| String::from(s))
    }

    pub fn as_str(&self, len: usize) -> Result<&'static str, &'static str> {
        assert!(self.check());
        core::str::from_utf8(self.as_slice(len)?).map_err(|_| "Invalid Utf8")
    }

    // 从一个 C 风格的零结尾字符串构造一个字符切片。
    /// Forms a zero-terminated string slice from a user pointer to a c style string.
    pub fn as_c_str(&self) -> Result<&'static str, &'static str> {
        self.as_str(unsafe { (0usize..).find(|&i| *self.ptr.add(i) == 0).unwrap() })
    }
}

impl<T, P: WritePolicy> UserPtr<T, P> {
    pub fn write(&mut self, value: T) {
        assert!(self.check());
        unsafe { copy_to_user(self.ptr, &value as *const T, 1) }
    }

    pub fn write_buf(&mut self, buf: &[T]) {
        assert!(self.check());
        unsafe { copy_to_user(self.ptr, buf.as_ptr(), buf.len()) };
    }
}
