#![no_std]
#![no_main]

use userstd::{
    println,
    sys::{self},
};

#[no_mangle]
fn main(_args: &[*const u8]) -> usize {
    println!("\n\nServos has booted sucessfully!");

    let sh = sys::spawn("/bin/sh", &[]).expect("init: couldn't spawn the shell!");
    sys::waitpid(sh).expect("init: shell process returned!");

    #[allow(clippy::empty_loop)]
    loop {}
}
