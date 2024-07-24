/*
https://www.lammertbies.nl/comm/info/serial-uart

            R                       W
base     RBR receiver buffer   THR transmitter holding | DLL divisor latch LSB  DLL divisor latch LSB
base+1   IER interrupt enable  IER interrupt enable    | DLM divisor latch MSB  DLM divisor latch MSB
base+2   IIR interrupt ident   FCR FIFO control        | IIR interrupt ident    FCR FIFO control
base+3   LCR line control      LCR line control        | LCR line control       LCR line control
base+4   MCR modem control     MCR modem control       | MCR modem control      MCR modem control
base+5   LSR line status       factory test            | LSR line status        factory test
base+6   MSR modem status      not used                | MSR modem status       not used
*/

use core::ptr::NonNull;

#[allow(clippy::upper_case_acronyms)]
#[allow(unused)]
enum Read {
    RBR = 0,
    IER = 1,
    IIR = 2,
    LCR = 3,
    MCR = 4,
    LSR = 5,
    MSR = 6,
}

#[allow(clippy::upper_case_acronyms)]
#[allow(unused)]
enum Write {
    THR = 0,
    IER = 1,
    FCR = 2,
    LCR = 3,
    MCR = 4,
}

pub struct Ns16550a {
    base: NonNull<u8>,
}

impl Ns16550a {
    /// Creates a new [`Ns16550a`].
    ///
    /// # Safety
    /// The `base` address must be a valid memory-mapped Ns16550a compliant UART controller.
    pub unsafe fn new(base: usize, _clock: u32) -> Ns16550a {
        unsafe {
            let this = Ns16550a {
                base: NonNull::new_unchecked(base as *mut u8),
            };

            // TODO: configure the divisor
            // this.write_reg(Write::FCR, 1 << 7); // enable divisor latch access
            // this.write_reg(Write::THR /* DLL */, v);
            // this.write_reg(Write::IER /* DLM */, v);

            this.write_reg(Write::LCR, 0b011); // 8-bit data size, 1 stop bit, no parity, disable divisor latch
            this.write_reg(Write::FCR, 0b1); // enable FIFO
            this.write_reg(Write::IER, 0b1); // enable receiver buffer interrupts
            this
        }
    }

    pub fn put(&mut self, byte: u8) {
        // wait for THR to be empty
        while self.read_reg(Read::LSR) & (1 << 5) == 0 {
            core::hint::spin_loop();
        }

        self.write_reg(Write::THR, byte);
    }

    pub fn read(&mut self) -> Option<u8> {
        if self.read_reg(Read::LSR) & 0b1 != 0 {
            Some(self.read_reg(Read::RBR))
        } else {
            None
        }
    }

    pub fn addr(&self) -> NonNull<u8> {
        self.base
    }

    #[inline(always)]
    fn read_reg(&self, reg: Read) -> u8 {
        unsafe { self.base.offset(reg as isize).read_volatile() }
    }

    #[inline(always)]
    fn write_reg(&self, reg: Write, v: u8) {
        unsafe { self.base.offset(reg as isize).write_volatile(v) }
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
