use core::ptr::NonNull;

pub use vaddr::*;
pub use paging::*;

mod paging;
mod vaddr;

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PhysAddr(pub usize);

impl From<usize> for PhysAddr {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

impl<T> From<*const T> for PhysAddr {
    fn from(value: *const T) -> Self {
        Self(value as usize)
    }
}

impl<T> From<*mut T> for PhysAddr {
    fn from(value: *mut T) -> Self {
        Self(value as usize)
    }
}

impl<T> From<*const [T]> for PhysAddr {
    fn from(value: *const [T]) -> Self {
        Self(value as *const T as usize)
    }
}

impl<T> From<*mut [T]> for PhysAddr {
    fn from(value: *mut [T]) -> Self {
        Self(value as *mut T as usize)
    }
}

impl<T> From<NonNull<T>> for PhysAddr {
    fn from(value: NonNull<T>) -> Self {
        Self(value.as_ptr() as usize)
    }
}
