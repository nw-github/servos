use core::{alloc::GlobalAlloc, cell::UnsafeCell, ptr::NonNull};

use linked_list_allocator::Heap;

struct UnsafeHeap(UnsafeCell<Heap>);

impl UnsafeHeap {
    pub fn with<T>(&self, f: impl FnOnce(&mut Heap) -> T) -> T {
        // Safety: there is no support for threading and GLOBAL_ALLOC is not directly accessible
        // outside of this file
        f(unsafe { &mut *self.0.get() })
    }
}

unsafe impl Sync for UnsafeHeap {}

unsafe impl GlobalAlloc for UnsafeHeap {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        self.with(|heap| {
            heap.allocate_first_fit(layout)
                .map(|p| p.as_ptr())
                .unwrap_or(core::ptr::null_mut())
        })
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
        self.with(|heap| unsafe { heap.deallocate(NonNull::new_unchecked(ptr), layout) })
    }
}

#[global_allocator]
static GLOBAL_ALLOC: UnsafeHeap = UnsafeHeap(UnsafeCell::new(Heap::empty()));

pub(crate) unsafe fn init(start: *mut u8, end: *mut u8) {
    GLOBAL_ALLOC.with(|heap| heap.init(start, end as usize - start as usize))
}

#[allow(unused)]
pub(crate) unsafe fn extend(sz: usize) {
    GLOBAL_ALLOC.with(|heap| heap.extend(sz))
}
