use servos::{
    drivers::Syscon,
    lock::SpinLocked,
    riscv::r_tp,
    sbi::{
        self,
        sys_reset::{ResetReason, ResetType},
    },
};

use crate::vmm::{VirtAddr, Page};

pub const MAX_HARTS: usize = 256;
pub const HART_STACK_LEN: usize = Page::SIZE * 4;

#[derive(Clone, Copy)]
pub struct HartInfo {
    /// The TOP of the stack
    pub sp: VirtAddr,
}

pub static mut HART_INFO: [HartInfo; MAX_HARTS] = [HartInfo { sp: VirtAddr(0) }; MAX_HARTS];

pub static POWER: SpinLocked<PowerManagement> =
    SpinLocked::new(PowerManagement::Sbi(SbiPowerManagement));

pub fn get_hart_info() -> HartInfo {
    unsafe { HART_INFO[r_tp()] }
}

#[allow(unused)]
pub fn set_hart_info(info: HartInfo) {
    unsafe {
        HART_INFO[r_tp()] = info;
    }
}

pub enum PowerManagement {
    Sbi(SbiPowerManagement),
    Syscon(Syscon),
}

impl PowerManagement {
    pub fn shutdown(&self) -> ! {
        match self {
            PowerManagement::Sbi(s) => s.shutdown(),
            PowerManagement::Syscon(s) => s.shutdown(),
        }
    }

    pub fn restart(&self) -> ! {
        match self {
            PowerManagement::Sbi(s) => s.restart(),
            PowerManagement::Syscon(s) => s.restart(),
        }
    }
}

pub struct SbiPowerManagement;

impl SbiPowerManagement {
    pub fn shutdown(&self) -> ! {
        _ = sbi::sys_reset::system_reset(ResetType::SHUTDOWN, ResetReason::NONE);
        panic!("return from SBI system reset");
    }

    pub fn restart(&self) -> ! {
        _ = sbi::sys_reset::system_reset(ResetType::COLD_REBOOT, ResetReason::NONE);
        panic!("return from SBI system reboot");
    }
}
