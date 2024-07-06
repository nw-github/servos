#![no_std]
#![no_main]
#![feature(naked_functions)]
#![feature(asm_const)]
#![feature(concat_idents)]
#![deny(unsafe_op_in_unsafe_fn)]

use core::{
    mem::MaybeUninit,
    sync::atomic::{AtomicBool, Ordering},
};

use config::{MAX_CPUS, STACK_LEN};
use servos::riscv;

mod config;
mod uart;

#[repr(C, align(16))]
pub struct Align16<T>(pub T);

static mut KSTACK: Align16<MaybeUninit<[[u8; STACK_LEN]; MAX_CPUS]>> =
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
extern "C" fn _start() {
    extern "C" fn init() -> ! {
        use riscv::*;

        // set return privilege level to supervisor
        w_mstatus(r_mstatus() & !MSTATUS_MPP_MASK | MSTATUS_MPP_S);
        w_mepc(kmain as usize); // set return address to main
        w_satp(0); // disable paging

        w_medeleg(0xffff); // delegate all exceptions to supervisor mode
        w_mideleg(0xffff); // delegate all interrupts to supervisor mode
        w_sie(r_sie() | SIE_STIE | SIE_SEIE | SIE_SSIE); // enable traps

        w_pmpaddr0(0x3fffffffffffff); // allow supervisor mode to access all memory
        w_pmpcfg0(0xf);

        w_tp(r_mhartid());

        unsafe { core::arch::asm!("mret", options(noreturn)) };
    }

    unsafe {
        core::arch::asm!(
            r"
            .option push
            .option norelax
            la      gp, _global_pointer
            .option pop

            csrr    a1, mhartid
            li      a2, {max_cpus}
            bgeu    a1, a2, 0

            la      sp, {stack}
            li      a0, {stack_len}
            addi    a1, a1, 1
            mul     a0, a0, a1
            add     sp, sp, a0
            tail    {init}

            0:
            wfi
            j       0
            ",

            init = sym init,
            stack = sym KSTACK,
            stack_len = const STACK_LEN,
            max_cpus = const MAX_CPUS,
            options(noreturn),
        );
    }
}

extern "C" fn kmain() -> ! {
    static INITIALIZED: AtomicBool = AtomicBool::new(false);

    let hartid = riscv::r_tp();
    if hartid == 0 {
        unsafe {
            uart::init(0x1000_0000);
        }

        INITIALIZED.store(true, Ordering::SeqCst);
    } else {
        while !INITIALIZED.load(Ordering::SeqCst) {
            core::hint::spin_loop();
        }
    }

    println!("Hello world from hart {}!", hartid);

    halt()
}
