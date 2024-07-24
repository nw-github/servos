use servos::{
    riscv::{
        disable_intr, enable_intr, r_sip, r_time, w_sie, w_sip, w_stvec, SIE_SEIE, SIE_SSIE,
        SIE_STIE,
    },
    sbi,
};

use crate::{plic::PLIC, println, riscv::r_scause};

const INTERRUPT_FLAG_BIT: usize = 1 << (usize::BITS - 1);

#[repr(usize)]
#[derive(strum::FromRepr, Debug)]
#[allow(clippy::enum_clike_unportable_variant)]
pub enum TrapCause {
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
    let _token = disable_intr();
    let cause = r_scause() & (INTERRUPT_FLAG_BIT | 0xff);
    match TrapCause::from_repr(cause) {
        Some(TrapCause::ExternalIntr) => {
            let irq = PLIC.hart_claim();
            println!("PLIC interrupt: {}", irq.0);
        }
        Some(TrapCause::TimerIntr) => {
            // println!("Timer!");
            _ = sbi::timer::set_timer(r_time() + 10_000_000);
        }
        Some(TrapCause::IllegalInstr) => panic!("Illegal instruction!"),
        Some(ex) => println!("[INFO] Unhandled trap: {ex:?}"),
        None => println!("[INFO] Unhandled trap: no match"),
    }

    let cause = cause & !INTERRUPT_FLAG_BIT;
    if cause != 0 {
        w_sip(r_sip() & !cause);
    }
}

pub fn install() {
    w_stvec(handle_trap as usize);
    w_sie(SIE_SEIE | SIE_STIE | SIE_SSIE);
    unsafe { enable_intr() };

    sbi::timer::set_timer(r_time() + 10_000_000).expect("SBI Timer support is not present");
}
