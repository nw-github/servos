use super::raw::{sbicall_1, sbicall_3, SbiResult};

pub const EXTENSION_ID: i32 = 0x4442434E;

pub fn write(bytes: impl AsRef<[u8]>) -> SbiResult<()> {
    let bytes = bytes.as_ref();
    sbicall_3(EXTENSION_ID, 0, bytes.len(), bytes.as_ptr() as usize, 0).into_result(|_| ())
}

pub fn read(bytes: &mut [u8]) -> SbiResult<usize> {
    sbicall_3(EXTENSION_ID, 1, bytes.len(), bytes.as_ptr() as usize, 0).into_result(|v| v as usize)
}

pub fn write_byte(byte: u8) -> SbiResult<()> {
    sbicall_1(EXTENSION_ID, 2, byte as usize).into_result(|_| ())
}
