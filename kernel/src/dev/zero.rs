use core::mem::MaybeUninit;

use crate::fs::{FsError, FsResult};

use super::Device;

pub struct ZeroDevice;

impl Device for ZeroDevice {
    fn read<'a>(&self, _pos: u64, buf: &'a mut [MaybeUninit<u8>]) -> FsResult<&'a mut [u8]> {
        buf.fill(MaybeUninit::new(0));
        Ok(unsafe { MaybeUninit::slice_assume_init_mut(buf) })
    }

    fn write(&self, _pos: u64, _buf: &[u8]) -> FsResult<usize> {
        Err(FsError::InvalidOp)
    }
}
