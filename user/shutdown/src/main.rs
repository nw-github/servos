#![no_std]
#![no_main]

use core::ffi::CStr;

use userstd::{println, sys};

#[no_mangle]
fn main(args: &[*const u8]) -> usize {
    let mut restart = false;
    for arg in args[1..]
        .iter()
        .map(|arg| unsafe { CStr::from_ptr(arg.cast()).to_bytes() })
    {
        if arg == b"-r" {
            restart = true;
        }
    }

    if let Err(err) = sys::shutdown(restart) {
        println!("error: {err:?}");
    }

    1
}
