use core::fmt::Write;

use servos::{drivers::Ns16550a, lock::SpinLocked, sbi};

pub enum DebugIo {
    Sbi(SbiConsole),
    Ns16550a(Ns16550a),
}

impl DebugIo {
    pub fn read(&mut self) -> Option<u8> {
        match self {
            DebugIo::Sbi(_) => SbiConsole::read(),
            DebugIo::Ns16550a(c) => c.read(),
        }
    }

    pub fn put(&mut self, byte: u8) {
        match self {
            DebugIo::Sbi(c) => c.put(byte),
            DebugIo::Ns16550a(c) => c.put(byte),
        }
    }
}

impl Write for DebugIo {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        match self {
            DebugIo::Sbi(c) => c.write_str(s),
            DebugIo::Ns16550a(c) => c.write_str(s),
        }
    }
}

pub static CONS: SpinLocked<DebugIo> = SpinLocked::new(DebugIo::Sbi(SbiConsole));

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

// from the std library
#[macro_export]
macro_rules! dbg {
    // NOTE: We cannot use `concat!` to make a static string as a format argument
    // of `eprintln!` because `file!` could contain a `{` or
    // `$val` expression could be a block (`{ .. }`), in which case the `eprintln!`
    // will be malformed.
    () => {
        $crate::println!("[{}:{}:{}]", file!(), line!(), column!())
    };
    ($val:expr $(,)?) => {
        // Use of `match` here is intentional because it affects the lifetimes
        // of temporaries - https://stackoverflow.com/a/48732525/1063961
        match $val {
            tmp => {
                $crate::println!("[{}:{}:{}] {} = {:#?}",
                    file!(), line!(), column!(), stringify!($val), &tmp);
                tmp
            }
        }
    };
    ($($val:expr),+ $(,)?) => {
        ($($crate::dbg!($val)),+,)
    };
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
