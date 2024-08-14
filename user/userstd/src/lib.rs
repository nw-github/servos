#![no_std]

pub mod print;
pub mod sys;

pub use shared::*;

#[panic_handler]
fn on_panic(info: &core::panic::PanicInfo) -> ! {
    println!("panic from init: {info}");

    _ = sys::kill(sys::getpid());
    loop {}
}
