use core::mem::MaybeUninit;

use crate::fs::FsResult;

pub mod console;
pub mod null;

pub trait Device {
    fn read<'a>(&self, pos: u64, buf: &'a mut [MaybeUninit<u8>]) -> FsResult<&'a mut [u8]>;
    fn write(&self, pos: u64, buf: &[u8]) -> FsResult<usize>;
}
