#![no_std]
#![no_main]

use core::ffi::CStr;

use userstd::{
    io::OpenFlags,
    println,
    sys::{self, RawFd, SysError},
};

#[no_mangle]
fn main(args: &[*const u8]) -> usize {
    let mut buf = [0; 0x4000];
    for arg in args[1..]
        .iter()
        .map(|arg| unsafe { CStr::from_ptr(arg.cast()).to_bytes() })
    {
        let strname = core::str::from_utf8(arg).unwrap();
        let fd = match sys::open(arg, OpenFlags::empty()) {
            Ok(fd) => fd,
            Err(SysError::PathNotFound) => {
                println!("'{strname}': doesn't exist");
                return 1;
            }
            Err(err) => {
                println!("'{strname}': read error: {err:?}");
                return 1;
            }
        };
        while let Ok(n) = sys::read(fd, None, &mut buf) {
            _ = sys::write(RawFd(0), None, &buf[..n]);
        }
    }

    0
}
