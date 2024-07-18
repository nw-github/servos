pub const MSTATUS_MPP_MASK: usize = 3 << 11; // previous mode.
pub const MSTATUS_MPP_M: usize = 3 << 11;
pub const MSTATUS_MPP_S: usize = 1 << 11;
pub const MSTATUS_MPP_U: usize = 0 << 11;
pub const MSTATUS_MIE: usize = 1 << 3; // machine-mode interrupt enable.

pub const SIE_SEIE: usize = 1 << 9; // external
pub const SIE_STIE: usize = 1 << 5; // timer
pub const SIE_SSIE: usize = 1 << 1; // software

pub const MIE_MEIE: usize = 1 << 11; // external
pub const MIE_MTIE: usize = 1 << 7; // timer
pub const MIE_MSIE: usize = 1 << 3; // software

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

status_reg_fns!(mstatus);
status_reg_fns!(satp);
status_reg_fns!(mscratch);
status_reg_fns!(mtvec);
status_reg_fns!(mepc);
status_reg_fns!(medeleg);
status_reg_fns!(mideleg);
status_reg_fns!(mie);
status_reg_fns!(sie);
status_reg_fns!(pmpaddr0);
status_reg_fns!(pmpcfg0);
read_register!(scause);
read_register!(stval);
read_register!(time);
read_register!(mhartid);

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
