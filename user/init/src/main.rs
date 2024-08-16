#![no_std]
#![no_main]

use userstd::{
    println,
    sys::{self},
};

#[no_mangle]
fn main(_args: &[*const u8]) -> usize {
    if sys::getpid() != 0 {
        println!("init process must be the first on the system");
        return 1;
    }

    println!("\n\nServos has booted sucessfully!");
    let sh = sys::spawn("/bin/sh", &[]).expect("init: couldn't spawn the shell!");
    sys::waitpid(sh).expect("init: shell process returned!");

    #[allow(clippy::empty_loop)]
    loop {}
}
