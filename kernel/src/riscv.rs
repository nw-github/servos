use core::{arch::asm, marker::PhantomData};

pub const SIE_SEIE: usize = 1 << 9; // external
pub const SIE_STIE: usize = 1 << 5; // timer
pub const SIE_SSIE: usize = 1 << 1; // software

pub const SSTATUS_SIE: usize = 1 << 1;
pub const SSTATUS_SPIE: usize = 1 << 5;
pub const SSTATUS_SPP: usize = 1 << 8;

macro_rules! read_register {
    ($name: ident) => {
        concat_idents::concat_idents!(read = r_, $name {
            #[inline(always)]
            #[must_use]
            pub fn read() -> usize {
                let val: usize;
                unsafe {
                    asm!(
                        concat!("csrr {val}, ", stringify!($name)),
                        val = out(reg) val,
                        options(nostack),
                    )
                };
                val
            }
        });
    };
}

macro_rules! status_reg_fns {
    ($name: ident) => {
        concat_idents::concat_idents!(write = w_, $name {
            #[inline(always)]
            pub fn write(val: usize) {
                unsafe {
                    asm!(
                        concat!("csrw ", stringify!($name), ", {val}"),
                        val = in(reg) val,
                        options(nostack),
                    )
                };
            }
        });

        read_register!($name);
    };
}

status_reg_fns!(satp);
status_reg_fns!(sie);
status_reg_fns!(stvec);
status_reg_fns!(sstatus);
status_reg_fns!(sip);
status_reg_fns!(sepc);
read_register!(scause);
read_register!(stval);
read_register!(time);

#[must_use]
#[inline(always)]
pub fn r_tp() -> usize {
    let val: usize;
    unsafe { asm!("mv {val}, tp", val = out(reg) val, options(nostack)) };
    val
}

pub struct InterruptToken {
    enabled: bool,
    _not_send_sync: PhantomData<*mut ()>,
}

impl InterruptToken {
    pub fn forget(self) {
        core::mem::forget(self);
    }

    pub fn was_enabled(&self) -> bool {
        self.enabled
    }
}

impl Drop for InterruptToken {
    fn drop(&mut self) {
        if self.enabled {
            unsafe { enable_intr() };
        }
    }
}

#[must_use]
#[inline(always)]
pub fn disable_intr() -> InterruptToken {
    let token = InterruptToken {
        enabled: r_sstatus() & SSTATUS_SIE != 0,
        _not_send_sync: PhantomData,
    };
    unsafe { asm!("csrc sstatus, {sie}", sie = const SSTATUS_SIE) };
    token
}

/// Enables interrupts on the current hart.
///
/// # Safety
///
/// This function isn't directly unsafe, but could lead to deadlocks or unsafety if interrupts are
/// enabled during a period they shouldn't be. Generally, this shouldn't be needed due to the drop
/// impl on InterruptToken.
#[inline(always)]
pub unsafe fn enable_intr() {
    unsafe { asm!("csrs sstatus, {sie}", sie = const SSTATUS_SIE) };
}
