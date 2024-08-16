#![no_std]
#![no_main]

use core::ffi::CStr;

use userstd::sys::{self, RawFd};

#[no_mangle]
fn main(args: &[*const u8]) -> usize {
    for (i, arg) in args[1..]
        .iter()
        .map(|arg| unsafe { CStr::from_ptr(arg.cast()).to_bytes() })
        .enumerate()
    {
        if i != 0 {
            _ = sys::write(RawFd(0), None, &[b' ']);
        }
        _ = sys::write(RawFd(0), None, arg);
    }
    _ = sys::write(RawFd(0), None, &[b'\n']);
    0
}
