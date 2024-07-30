use super::raw::{sbicall_0, sbicall_1, sbicall_3, SbiResult};

#[repr(usize)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HartState {
    /// The hart is physically powered-up and executing normally.
    Started,
    /// The hart is not executing in supervisor-mode or any lower privilege mode. It is probably powered-down by the SBI implementation if the underlying platform has a mechanism to physically power-down harts.
    Stopped,
    /// Some other hart has requested to start (or power-up) the hart from the STOPPED state and the SBI implementation is still working to get the hart in the STARTED state.
    StartPending,
    /// The hart has requested to stop (or power-down) itself from the STARTED state and the SBI implementation is still working to get the hart in the STOPPED state.
    StopPending,
    /// This hart is in a platform specific suspend (or low power) state.
    Suspended,
    /// The hart has requested to put itself in a platform specific low power state from the STARTED state and the SBI implementation is still working to get the hart in the platform specific SUSPENDED state.
    SuspendPending,
    /// An interrupt or platform specific hardware event has caused the hart to resume normal execution from the SUSPENDED state and the SBI implementation is still working to get the hart in the STARTED state.
    ResumePending,
}

pub const EXTENSION_ID: i32 = 0x48534D;

pub fn hart_start(
    hartid: usize,
    start_addr: extern "C" fn(hartid: usize, opaque: usize),
    opaque: usize,
) -> SbiResult<()> {
    sbicall_3(EXTENSION_ID, 0, hartid, start_addr as usize, opaque).into_result(|_| ())
}

pub fn hart_stop() -> SbiResult<()> {
    sbicall_0(EXTENSION_ID, 1).into_result(|_| ())
}

pub fn hart_get_status(hartid: usize) -> SbiResult<HartState> {
    sbicall_1(EXTENSION_ID, 2, hartid).into_result(|r| {
        assert!(matches!(r, 0..=6), "SBI returned invalid HartState");
        unsafe { core::mem::transmute::<isize, HartState>(r) }
    })
}

// TODO: sbi_hart_suspend
