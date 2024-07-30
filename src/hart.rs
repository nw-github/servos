use servos::riscv::r_tp;

pub const MAX_HARTS: usize = 256;
pub const HART_STACK_LEN: usize = 0x4000;

#[derive(Clone, Copy)]
pub struct HartInfo {
    /// The TOP of the stack
    pub sp: usize,
}

static mut HART_STATE: [HartInfo; MAX_HARTS] = [HartInfo { sp: 0 }; MAX_HARTS];

pub fn get_hart_info() -> HartInfo {
    unsafe { HART_STATE[r_tp()] }
}

pub fn set_hart_info(info: HartInfo) {
    unsafe {
        HART_STATE[r_tp()] = info;
    }
}
