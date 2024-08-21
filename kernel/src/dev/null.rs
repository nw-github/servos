use core::mem::MaybeUninit;

use crate::fs::{FsError, FsResult};

use super::Device;

pub struct NullDevice;

impl Device for NullDevice {
    fn read<'a>(&self, _pos: u64, _buf: &'a mut [MaybeUninit<u8>]) -> FsResult<&'a mut [u8]> {
        Err(FsError::Eof)
    }

    fn write(&self, _pos: u64, _buf: &[u8]) -> FsResult<usize> {
        Err(FsError::InvalidOp)
    }
}
