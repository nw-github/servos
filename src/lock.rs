use core::sync::atomic::{AtomicBool, Ordering};

use lock_api::GuardSend;

pub struct SpinLock(AtomicBool);

unsafe impl lock_api::RawMutex for SpinLock {
    #[allow(clippy::declare_interior_mutable_const)]
    const INIT: Self = Self(AtomicBool::new(false));

    type GuardMarker = GuardSend;

    fn lock(&self) {
        // TODO: disable interrupts
        while self
            .0
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            while self.0.load(Ordering::Relaxed) {
                core::hint::spin_loop();
            }
        }
    }

    fn try_lock(&self) -> bool {
        self.0
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
    }

    unsafe fn unlock(&self) {
        self.0.store(false, Ordering::Release)
    }
}

pub type SpinLocked<T> = lock_api::Mutex<SpinLock, T>;
