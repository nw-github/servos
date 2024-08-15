use core::fmt::Write;

use crate::sys::{self, RawFd};

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
        _ = write!($crate::print::Stdout, $($arg)*);
    });
}

#[macro_export]
macro_rules! println {
    ($($arg: tt)*) => ({
        use core::fmt::Write;
        _ = writeln!($crate::print::Stdout, $($arg)*);
    });
}

