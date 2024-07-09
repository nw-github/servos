pub struct Ns16550a {
    base: *mut u8,
}

impl Ns16550a {
    /// Creates a new [`Ns16550a`].
    ///
    /// # Safety
    /// The `base` address must be a valid memory-mapped Ns16550a compliant UART controller.
    pub unsafe fn new(base: usize) -> Ns16550a {
        let base = base as *mut u8;
        unsafe {
            base.offset(3).write_volatile(0b11); // 8-bit data size
            base.offset(2).write_volatile(1); // enable FIFO
            // base.offset(1).write_volatile(1); // enable receiver buffer interrupts
        }
        Ns16550a { base }
    }

    pub fn put(&mut self, byte: u8) {
        unsafe { self.base.write_volatile(byte) };
    }
}

impl core::fmt::Write for Ns16550a {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for byte in s.bytes() {
            if byte == b'\n' {
                self.put(b'\r');
            }
            self.put(byte);
        }

        Ok(())
    }
}
