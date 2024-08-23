#![no_std]
#![no_main]

use core::ffi::CStr;

use userstd::{
    println,
    sys::{self},
};

#[no_mangle]
fn main(args: &[*const u8]) -> usize {
    for arg in args[1..]
        .iter()
        .map(|arg| unsafe { CStr::from_ptr(arg.cast()) })
    {
        let Some(pid) = core::str::from_utf8(arg.to_bytes())
            .ok()
            .and_then(|a| a.parse::<u32>().ok())
        else {
            println!("{arg:?}: couldn't parse to pid");
            continue;
        };

        match sys::kill(pid) {
            Ok(()) => println!("pid {pid}: killed"),
            Err(err) => println!("pid {pid}: error: {err:?}"),
        }
    }
    0
}
