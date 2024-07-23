use servos::{
    riscv::{disable_intr, enable_intr, r_time, w_sie, w_stvec, SIE_SEIE, SIE_SSIE, SIE_STIE},
    sbi,
};

use crate::{println, riscv::r_scause};

const INTERRUPT_FLAG_BIT: usize = 1 << (usize::BITS - 1);

#[repr(usize)]
#[derive(strum::FromRepr, Debug)]
#[allow(clippy::enum_clike_unportable_variant)]
pub enum TrapCause {
    Software = 1 | INTERRUPT_FLAG_BIT,
    Timer = 5 | INTERRUPT_FLAG_BIT,
    External = 9 | INTERRUPT_FLAG_BIT,
    CounterOverflow = 13 | INTERRUPT_FLAG_BIT,

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
    let scause = r_scause();
    let _token = disable_intr();
    match TrapCause::from_repr(scause) {
        Some(TrapCause::Timer) => {
            println!("Timer!");
            _ = sbi::timer::set_timer(r_time() + 10_000_000);
        }
        Some(TrapCause::IllegalInstr) => panic!("Illegal instruction!"),
        Some(ex) => println!("[INFO] Unhandled trap: {ex:?}"),
        None => println!("[INFO] Unhandled trap: no match"),
    }
}

pub fn install() {
    w_stvec(handle_trap as usize);
    w_sie(SIE_SEIE | SIE_STIE | SIE_SSIE);
    unsafe { enable_intr() };

    sbi::timer::set_timer(r_time() + 10_000_000).expect("SBI Timer support is not present");
}
