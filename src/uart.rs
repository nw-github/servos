use servos::{lock::SpinLocked, sbi};

pub static CONS: SpinLocked<SbiConsole> = SpinLocked::new(SbiConsole);

#[macro_export]
macro_rules! print {
    ($($arg: tt)*) => ({
        use core::fmt::Write;
        _ = write!($crate::uart::CONS.lock(), $($arg)*);
    });
}

#[macro_export]
macro_rules! println {
    ($($arg: tt)*) => ({
        use core::fmt::Write;
        _ = writeln!($crate::uart::CONS.lock(), $($arg)*);
    });
}

pub struct SbiConsole;

impl SbiConsole {
    pub fn put(&mut self, byte: u8) {
        _ = sbi::debug_console::write_byte(byte);
    }

    pub fn read() -> Option<u8> {
        let mut buf = 0;
        if let Ok(1) = sbi::debug_console::read(core::slice::from_mut(&mut buf)) {
            Some(buf)
        } else {
            None
        }
    }
}

impl core::fmt::Write for SbiConsole {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        _ = sbi::debug_console::write(s);
        Ok(())
    }
}
