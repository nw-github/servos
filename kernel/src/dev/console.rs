use core::mem::MaybeUninit;

use crate::fs::FsResult;

use super::Device;

pub struct Console;

impl Device for Console {
    fn read<'a>(&self, _pos: u64, buf: &'a mut [MaybeUninit<u8>]) -> FsResult<&'a mut [u8]> {
        buf.fill(MaybeUninit::new(0));
        Ok(unsafe { MaybeUninit::slice_assume_init_mut(buf) })
    }

    fn write(&self, _pos: u64, buf: &[u8]) -> FsResult<usize> {
        let mut cons = crate::uart::CONS.lock();
        buf.iter().for_each(|&b| cons.put(b));
        Ok(buf.len())
    }
}
