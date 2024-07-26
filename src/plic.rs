use core::{cell::UnsafeCell, num::NonZeroU32};

pub struct Plic(UnsafeCell<*mut u8>);

unsafe impl Sync for Plic {}

pub static PLIC: Plic = Plic::new();

const PRIORITY_BASE: usize = 0;
// const PENDING_BASE: usize    = 0x001000;
const ENABLE_BASE: usize = 0x002000;
const THRESHOLDS_BASE: usize = 0x200000;
const CLAIM_BASE: usize = 0x200000 + 4;
const COMPLETION_BASE: usize = CLAIM_BASE;

impl Plic {
    pub const fn new() -> Self {
        Self(UnsafeCell::new(core::ptr::null_mut()))
    }

    /// Creates a new [`Plic`].
    ///
    /// # Safety
    ///
    /// `addr` must be the address of a standard-compliant RISC-V PLIC. No other harts must be
    /// started yet, and no other PLIC functions should be called before this.
    pub unsafe fn init(&self, addr: *mut u8) {
        unsafe {
            *self.0.get() = addr;
        }
    }

    pub unsafe fn set_priority(&self, src: u32, priority: u32) {
        debug_assert!((1..1024).contains(&src), "src is {src}");
        unsafe {
            *self.u32(PRIORITY_BASE + src as usize * 4) = priority;
        }
    }

    pub fn hart_enable(&self, src: u32) {
        debug_assert!((1..1024).contains(&src), "src is {src}");
        unsafe {
            *self.u32(ENABLE_BASE + (src as usize / 32) * 4 + Self::s_ctx_offset(0x80)) =
                1 << (src % 32);
        }
    }

    pub fn set_hart_priority_threshold(&self, priority: u32) {
        unsafe {
            *self.u32(THRESHOLDS_BASE + Self::s_ctx_offset(0x1000)) = priority;
        }
    }

    #[must_use]
    pub fn hart_claim(&self) -> Irq {
        unsafe {
            Irq(NonZeroU32::new(
                *self.u32(CLAIM_BASE + Self::s_ctx_offset(0x1000)),
            ))
        }
    }

    pub fn addr(&self) -> *mut u8 {
        unsafe { *self.0.get() }
    }

    fn hart_complete(&self, irq: u32) {
        unsafe { *self.u32(COMPLETION_BASE + Self::s_ctx_offset(0x1000)) = irq };
    }

    fn u32(&self, byte_offset: usize) -> *mut u32 {
        debug_assert!(byte_offset & 0b11 == 0);

        let ptr = unsafe { *self.0.get() };
        assert!(!ptr.is_null());
        unsafe { ptr.add(byte_offset).cast() }
    }

    #[inline(always)]
    fn s_ctx_offset(offset: usize) -> usize {
        // Even contexts are M-mode
        offset + crate::riscv::r_tp() * offset * 2
    }
}

pub struct Irq(Option<NonZeroU32>);

impl Irq {
    pub fn value(&self) -> Option<&NonZeroU32> {
        self.0.as_ref()
    }
}

impl Drop for Irq {
    fn drop(&mut self) {
        if let Some(v) = self.0 {
            PLIC.hart_complete(v.into());
        }
    }
}
