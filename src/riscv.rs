use core::marker::PhantomData;

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
                unsafe { core::arch::asm!(concat!("csrr {val}, ", stringify!($name)), val = out(reg) val) };
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
                unsafe { core::arch::asm!(concat!("csrw ", stringify!($name), ", {val}"), val = in(reg) val) };
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
    unsafe { core::arch::asm!("mv {val}, tp", val = out(reg) val) };
    val
}

#[inline(always)]
pub fn sfence_vma() {
    unsafe { core::arch::asm!("sfence.vma zero, zero") };
}

#[inline(always)]
pub fn halt() -> ! {
    loop {
        unsafe {
            core::arch::asm!("wfi");
        }
    }
}

pub struct InterruptToken {
    enabled: bool,
    _not_send_sync: PhantomData<*mut ()>,
}

impl InterruptToken {
    pub fn forget(self) {
        core::mem::forget(self);
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
    unsafe { core::arch::asm!("csrc sstatus, {sie}", sie = const SSTATUS_SIE) };
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
    unsafe { core::arch::asm!("csrs sstatus, {sie}", sie = const SSTATUS_SIE) };
}
