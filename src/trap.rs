use core::num::NonZeroU32;

use servos::{
    riscv::{enable_intr, r_time, w_sie, w_stvec, SIE_SEIE, SIE_SSIE, SIE_STIE},
    sbi,
};

use crate::{plic::PLIC, println, riscv::r_scause, uart::CONS};

const INTERRUPT_FLAG_BIT: usize = 1 << (usize::BITS - 1);

struct TrapContext {
    uart_irq: Option<NonZeroU32>,
}

static mut TRAP_CONTEXT: TrapContext = TrapContext { uart_irq: None };

#[repr(usize)]
#[derive(strum::FromRepr, Debug)]
#[allow(clippy::enum_clike_unportable_variant)]
enum TrapCause {
    SoftwareIntr = 1 | INTERRUPT_FLAG_BIT,
    TimerIntr = 5 | INTERRUPT_FLAG_BIT,
    ExternalIntr = 9 | INTERRUPT_FLAG_BIT,
    CounterOverflowIntr = 13 | INTERRUPT_FLAG_BIT,

    InstrAddrMisaligned = 0,
    InstrAccessFault,
    IllegalInstr,
    Breakpoint,
    LoadMisaligned,
    LoadAccessFault,
    StoreAddrMisaligned,
    StoreAccessFault,
    EcallUMode,
    EcallSMode,
    InstrPageFault = 12,
    LoadPageFault,
    StorePageFault = 15,
    SoftwareCheck = 18,
    HardwareError = 19,
}

#[repr(align(4))]
extern "riscv-interrupt-s" fn handle_trap() {
    let cause = r_scause() & (INTERRUPT_FLAG_BIT | 0xff);
    match TrapCause::from_repr(cause) {
        Some(TrapCause::ExternalIntr) => {
            let irq = PLIC.hart_claim();
            let Some(num) = irq.value() else {
                println!("PLIC interrupt with null irq");
                return;
            };

            if unsafe { TRAP_CONTEXT.uart_irq.as_ref() }.is_some_and(|v| v == num) {
                let ch = CONS.lock().read().unwrap();
                println!("UART interrupt: {ch:#04x}");
            } else {
                println!("PLIC interrupt with unknown irq {num}");
            }
        }
        Some(TrapCause::TimerIntr) => {
            println!("Timer!");
            _ = sbi::timer::set_timer(r_time() + 10_000_000);
        }
        Some(TrapCause::IllegalInstr) => panic!("Illegal instruction!"),
        Some(ex) => println!("[INFO] Unhandled trap: {ex:?}"),
        None => println!("[INFO] Unhandled trap: no match"),
    }
}

pub unsafe fn init_context(uart_irq: Option<u32>) {
    unsafe {
        TRAP_CONTEXT.uart_irq = uart_irq.and_then(NonZeroU32::new);
    }
}

pub fn hart_install() {
    w_stvec(handle_trap as usize);
    w_sie(SIE_SEIE | SIE_STIE | SIE_SSIE);
    unsafe { enable_intr() };

    sbi::timer::set_timer(r_time() + 10_000_000).expect("SBI Timer support is not present");
}
