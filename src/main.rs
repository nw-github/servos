#![no_std]
#![no_main]
#![feature(naked_functions)]
#![feature(asm_const)]
#![deny(unsafe_op_in_unsafe_fn)]

use core::{mem::MaybeUninit, ptr::addr_of};

use config::{HART_STACK_LEN, MAX_CPUS};
use servos::riscv;
use uart::SbiConsole;

mod config;
mod uart;

#[repr(C, align(16))]
pub struct Align16<T>(pub T);

static mut KSTACK: Align16<MaybeUninit<[[u8; HART_STACK_LEN]; MAX_CPUS]>> =
    Align16(MaybeUninit::uninit());

#[inline(always)]
fn halt() -> ! {
    loop {
        unsafe {
            core::arch::asm!("wfi");
        }
    }
}

#[panic_handler]
fn on_panic(info: &core::panic::PanicInfo) -> ! {
    println!("panic: {}", info);
    halt()
}

#[naked]
#[no_mangle]
#[link_section = ".text.init"]
extern "C" fn _start() -> ! {
    unsafe {
        core::arch::asm!(
            r"
            .option push
            .option norelax
            la      gp, _global_pointer
            .option pop

            mv      tp, a0
            la      sp, {stack}
            li      t0, {stack_len}
            addi    t1, a0, 1
            mul     t0, t0, t1
            add     sp, sp, t0
            tail    {init}",

            init = sym kmain,
            stack = sym KSTACK,
            stack_len = const HART_STACK_LEN,
            options(noreturn),
        );
    }
}

extern "C" {
    static _kernel_end: u8;
}

extern "C" fn kmain(hartid: usize, arg1: *mut u8) -> ! {
    println!(
        "\n\nHello world from kernel hart {}!\ntp: {:#x}\narg1: {:?}\n_kernel_end: {:?}",
        hartid,
        riscv::r_tp(),
        arg1,
        unsafe { addr_of!(_kernel_end) },
    );

    loop {
        if let Some(ch) = SbiConsole::read() {
            println!("{:#02x}: {}", ch, ch as char);
        }
    }
}
