use crate::fs::FsResult;

pub mod console;
pub mod null;

pub trait Device {
    fn read(&self, pos: u64, buf: &mut [u8]) -> FsResult<usize>;
    fn write(&self, pos: u64, buf: &[u8]) -> FsResult<usize>;
}
