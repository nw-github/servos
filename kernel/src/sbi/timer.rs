use super::raw::{sbicall_1, SbiResult};

pub const EXTENSION_ID: i32 = 0x54494D45;

// should be u64?
pub fn set_timer(stime_value: usize) -> SbiResult<()> {
    sbicall_1(EXTENSION_ID, 0, stime_value).into_result(|_| ())
}
