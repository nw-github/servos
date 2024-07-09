use super::raw::{sbicall_1, sbicall_3, SbiResult};

pub fn write(bytes: impl AsRef<[u8]>) -> SbiResult<()> {
    let bytes = bytes.as_ref();
    sbicall_3(0x4442434E, 0, bytes.len(), bytes.as_ptr() as usize, 0).into_result(|_| ())
}

pub fn read(bytes: &mut [u8]) -> SbiResult<usize> {
    sbicall_3(0x4442434E, 1, bytes.len(), bytes.as_ptr() as usize, 0).into_result(|v| v as usize)
}

pub fn write_byte(byte: u8) -> SbiResult<()> {
    sbicall_1(0x4442434E, 2, byte as usize).into_result(|_| ())
}
