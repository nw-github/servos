use crate::riscv::halt;

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
        halt()
    }

    pub fn restart(&self) -> ! {
        unsafe { self.base.write_volatile(self.restart_magic) };
        halt()
    }

    pub fn base(&self) -> *mut u32 {
        self.base
    }
}
