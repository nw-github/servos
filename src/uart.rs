use servos::{lock::SpinLocked, sbi};

pub static CONS: SpinLocked<Option<SbiConsole>> = SpinLocked::new(Some(SbiConsole));

#[macro_export]
macro_rules! print {
    ($($arg: tt)*) => ({
        use core::fmt::Write;
        $crate::uart::CONS.lock().as_mut().map(|writer| {
            _ = writer.write_fmt(format_args!($($arg)*));
        });
    });
}

#[macro_export]
macro_rules! println {
    ($fmt: expr) => ($crate::print!(concat!($fmt, "\n")));
    ($fmt: expr, $($arg: tt)*) => ($crate::print!(concat!($fmt, "\n"), $($arg)*));
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
