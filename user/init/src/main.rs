#![no_std]
#![no_main]

#[panic_handler]
fn on_panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

fn _start() {
    loop {}
}
