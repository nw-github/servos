use super::raw::{sbicall_2, SbiResult};

pub const EXTENSION_ID: i32 = 0x53525354;

pub struct ResetType(pub u32);

impl ResetType {
    pub const SHUTDOWN: ResetType = ResetType(0);
    pub const COLD_REBOOT: ResetType = ResetType(1);
    pub const WARM_REBOOT: ResetType = ResetType(2);
}

pub struct ResetReason(pub u32);

impl ResetReason {
    pub const NONE: ResetReason = ResetReason(0);
    pub const SYSTEM_FAILURE: ResetReason = ResetReason(1);
}

pub fn system_reset(typ: ResetType, reason: ResetReason) -> SbiResult<()> {
    sbicall_2(EXTENSION_ID, 0, typ.0 as usize, reason.0 as usize).into_result(|_| ())
}
