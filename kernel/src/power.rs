use servos::{
    drivers::Syscon,
    lock::SpinLocked,
    sbi::{
        self,
        sys_reset::{ResetReason, ResetType},
    },
};

pub static POWER: SpinLocked<PowerManagement> =
    SpinLocked::new(PowerManagement::Sbi(SbiPowerManagement));

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
