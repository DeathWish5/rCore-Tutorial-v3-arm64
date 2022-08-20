//!Stdin & Stdout
use super::File;
use crate::arch::console_getchar;
use crate::task::{CurrentTask, SignalFlags};

///Standard input
pub struct Stdin;
///Standard output
pub struct Stdout;

impl File for Stdin {
    fn readable(&self) -> bool {
        true
    }
    fn writable(&self) -> bool {
        false
    }
    fn read(&self, buf: &mut [u8]) -> usize {
        assert_eq!(buf.len(), 1);
        // busy loop
        let ch = loop {
            match console_getchar() {
                None => {
                    CurrentTask::get().yield_now();
                }
                // 3 is Crtl-C
                Some(3) => {
                    CurrentTask::get().set_singal(SignalFlags::SIGINT);
                    break 3;
                }
                Some(c) => {
                    break c;
                }
            }
        };
        buf[0] = ch;
        1
    }
    fn write(&self, _buf: &[u8]) -> usize {
        panic!("Cannot write to stdin!");
    }
}

impl File for Stdout {
    fn readable(&self) -> bool {
        false
    }
    fn writable(&self) -> bool {
        true
    }
    fn read(&self, _buf: &mut [u8]) -> usize {
        panic!("Cannot read from stdout!");
    }
    fn write(&self, buf: &[u8]) -> usize {
        print!("{}", core::str::from_utf8(buf).unwrap());
        buf.len()
    }
}
