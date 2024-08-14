use super::Device;

pub struct NullDevice;

impl Device for NullDevice {
    fn read(&self, _pos: u64, buf: &mut [u8]) -> crate::fs::FsResult<usize> {
        buf.fill(0);
        Ok(buf.len())
    }

    fn write(&self, _pos: u64, buf: &[u8]) -> crate::fs::FsResult<usize> {
        Ok(buf.len())
    }
}
