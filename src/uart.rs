use core::cell::OnceCell;

use servos::lock::SpinLocked;

pub struct Ns16550a {
    base: *mut u8,
}

impl Ns16550a {
    pub unsafe fn new(base: usize) -> Ns16550a {
        let base = base as *mut u8;
        unsafe {
            base.offset(3).write_volatile(0b11); // 8-bit data size
            base.offset(2).write_volatile(1); // enable FIFO
            // base.offset(1).write_volatile(1); // enable receiver buffer interrupts
        }
        Ns16550a { base }
    }

    pub fn put(&mut self, ch: u8) {
        unsafe { self.base.write_volatile(ch) };
    }
}

impl core::fmt::Write for Ns16550a {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for byte in s.bytes() {
            self.put(byte);
        }

        Ok(())
    }
}

pub static CONS: SpinLocked<OnceCell<Ns16550a>> = SpinLocked::new(OnceCell::new());

pub unsafe fn init(base: usize) {
    CONS.lock().get_or_init(|| unsafe { Ns16550a::new(base) });
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ({
        use core::fmt::Write;
        $crate::uart::CONS.lock().get_mut().map(|writer| {
            _ = writer.write_fmt(format_args!($($arg)*));
        });
    });
}

#[macro_export]
macro_rules! println {
    ($fmt:expr) => ($crate::print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => ($crate::print!(concat!($fmt, "\n"), $($arg)*));
}
