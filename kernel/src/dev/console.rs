use super::Device;

pub struct Console;

impl Device for Console {
    fn read(&self, _pos: u64, buf: &mut [u8]) -> crate::fs::FsResult<usize> {
        buf.fill(0);
        Ok(buf.len())
    }

    fn write(&self, _pos: u64, buf: &[u8]) -> crate::fs::FsResult<usize> {
        let mut cons = crate::uart::CONS.lock();
        buf.iter().for_each(|&b| cons.put(b));
        Ok(buf.len())
    }
}
