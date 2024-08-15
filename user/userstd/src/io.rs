use core::fmt::Write;

use crate::sys::{self, RawFd};

use alloc::vec::Vec;
pub use shared::io::*;
use shared::sys::SysError;

pub struct Stdout;

impl Write for Stdout {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        _ = sys::write(RawFd(0), None, s.as_bytes());
        Ok(())
    }
}

#[macro_export]
macro_rules! print {
    ($($arg: tt)*) => ({
        use core::fmt::Write;
        _ = write!($crate::io::Stdout, $($arg)*);
    });
}

#[macro_export]
macro_rules! println {
    ($($arg: tt)*) => ({
        use core::fmt::Write;
        _ = writeln!($crate::io::Stdout, $($arg)*);
    });
}

pub fn read_file(path: &[u8]) -> Result<Vec<u8>, SysError> {
    let fd = sys::open(path, OpenFlags::empty())?;
    let buf = read_fd(fd)?;
    _ = sys::close(fd);
    Ok(buf)
}

pub fn read_fd(fd: RawFd) -> Result<Vec<u8>, SysError> {
    let stat = sys::stat(fd)?;
    let mut buf = alloc::vec![0; stat.size];
    let n = sys::read(fd, None, &mut buf)?;
    buf.truncate(n);
    Ok(buf)
}
