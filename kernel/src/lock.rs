use core::{
    cell::UnsafeCell,
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicBool, Ordering},
};

use crate::riscv::{disable_intr, InterruptToken};

pub struct SpinLocked<T> {
    data: UnsafeCell<T>,
    locked: AtomicBool,
}

unsafe impl<T> Sync for SpinLocked<T> {}

impl<T> SpinLocked<T> {
    pub const fn new(data: T) -> Self {
        Self {
            data: UnsafeCell::new(data),
            locked: AtomicBool::new(false),
        }
    }

    pub fn lock(&self) -> Guard<T> {
        let token = disable_intr();
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            while self.locked.load(Ordering::Relaxed) {
                core::hint::spin_loop();
            }
        }

        Guard::new(token, self)
    }

    pub fn try_lock(&self) -> Option<Guard<T>> {
        let token = disable_intr();
        self.locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .ok()
            .map(|_| Guard::new(token, self))
    }

    pub fn with<U>(&self, f: impl FnOnce(Guard<T>) -> U) -> U {
        f(self.lock())
    }

    unsafe fn unlock(&self) {
        self.locked.store(false, Ordering::Release)
    }

    pub const fn as_ptr(&self) -> *mut T {
        self.data.get()
    }
}

pub struct Guard<'a, T> {
    lock: &'a SpinLocked<T>,
    token: ManuallyDrop<InterruptToken>,
}

impl<'a, T> Guard<'a, T> {
    pub fn new(token: InterruptToken, lock: &'a SpinLocked<T>) -> Self {
        Self {
            lock,
            token: ManuallyDrop::new(token),
        }
    }

    pub fn interrupt_token<'g>(this: &'g Guard<'a, T>) -> &'g InterruptToken {
        &this.token
    }

    pub fn drop_and_keep_token(this: Guard<'a, T>) -> InterruptToken {
        unsafe {
            this.lock.unlock();
            Guard::forget_and_keep_token(this)
        }
    }

    pub fn forget_and_keep_token(mut this: Guard<'a, T>) -> InterruptToken {
        unsafe {
            let token = ManuallyDrop::take(&mut this.token);
            core::mem::forget(this);
            token
        }
    }
}

impl<T> Drop for Guard<'_, T> {
    fn drop(&mut self) {
        unsafe {
            self.lock.unlock();
            ManuallyDrop::drop(&mut self.token)
        }
    }
}

impl<T> Deref for Guard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T> DerefMut for Guard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.data.get() }
    }
}
