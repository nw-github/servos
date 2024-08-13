#![no_std]
#![no_main]

#[panic_handler]
fn on_panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

fn syscall(no: usize, a0: usize, a1: usize, a2: usize) -> isize {
    let result;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") no,
            in("a0") a0,
            in("a1") a1,
            in("a2") a2,
            lateout("a0") result,
        );
    }
    result
}

pub fn _start() {
    syscall(1, 0, 0, 0);

    loop {}
}
