use core::mem::MaybeUninit;

use crate::fs::FsResult;

use super::Device;

pub struct NullDevice;

impl Device for NullDevice {
    fn read<'a>(&self, _pos: u64, buf: &'a mut [MaybeUninit<u8>]) -> FsResult<&'a mut [u8]> {
        buf.fill(MaybeUninit::new(0));
        Ok(unsafe { MaybeUninit::slice_assume_init_mut(buf) })
    }

    fn write(&self, _pos: u64, buf: &[u8]) -> FsResult<usize> {
        Ok(buf.len())
    }
}
