pub struct Syscon {
    base: *mut u32,
    shutdown_magic: u32,
    restart_magic: u32,
}

impl Syscon {
    /// .
    ///
    /// # Safety
    ///
    /// .
    pub unsafe fn init_with_magic(base: *mut u32, shutdown_magic: u32, restart_magic: u32) -> Self {
        Self {
            base,
            shutdown_magic,
            restart_magic,
        }
    }

    pub fn shutdown(&self) -> ! {
        unsafe { self.base.write_volatile(self.shutdown_magic) };
        panic!("return after writing to syscon register");
    }

    pub fn restart(&self) -> ! {
        unsafe { self.base.write_volatile(self.restart_magic) };
        panic!("return after writing to syscon register");
    }

    pub fn addr(&self) -> *mut u32 {
        self.base
    }
}
