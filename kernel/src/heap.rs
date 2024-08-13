use core::{
    alloc::{GlobalAlloc, Layout},
    mem::MaybeUninit,
    ops::Range,
    ptr::NonNull,
};

use linked_list_allocator::Heap;

use crate::lock::SpinLocked;

pub struct BlockAlloc {
    blocks: [Option<&'static mut Node>; BLOCK_SIZES.len()],
    fallback: Heap,
}

impl BlockAlloc {
    pub const fn new() -> Self {
        Self {
            blocks: [const { None }; BLOCK_SIZES.len()],
            fallback: Heap::empty(),
        }
    }

    pub fn init(&mut self, heap: &'static mut [MaybeUninit<u8>]) {
        self.fallback.init_from_slice(heap)
    }

    pub fn alloc(&mut self, layout: Layout) -> *mut u8 {
        if let Some(i) = Self::list_index(&layout) {
            let Some(block) = self.blocks[i].take() else {
                // Safety: all BLOCK_SIZES are powers of two
                return self.fallback_alloc(unsafe {
                    Layout::from_size_align_unchecked(BLOCK_SIZES[i], BLOCK_SIZES[i])
                });
            };

            self.blocks[i] = block.next.take();
            block as *mut _ as *mut u8
        } else {
            self.fallback_alloc(layout)
        }
    }

    /// .
    ///
    /// # Safety
    ///
    /// .
    pub unsafe fn dealloc(&mut self, ptr: NonNull<u8>, layout: Layout) {
        if let Some(i) = Self::list_index(&layout) {
            // Safety: every block has sufficient size + alignment for a Node
            let block = unsafe { ptr.cast::<Node>().as_mut() };
            block.next = self.blocks[i].take();
            self.blocks[i] = Some(block);
        } else {
            unsafe { self.fallback.deallocate(ptr, layout) };
        }
    }

    pub fn range(&self) -> Range<*mut u8> {
        Range {
            start: self.fallback.bottom(),
            end: self.fallback.top(),
        }
    }

    fn list_index(layout: &Layout) -> Option<usize> {
        let min = layout.size().max(layout.align());
        BLOCK_SIZES.iter().position(|&s| s >= min)
    }

    fn fallback_alloc(&mut self, layout: Layout) -> *mut u8 {
        self.fallback
            .allocate_first_fit(layout)
            .map(|ptr| ptr.as_ptr())
            .unwrap_or(core::ptr::null_mut())
    }
}

impl Default for BlockAlloc {
    fn default() -> Self {
        Self::new()
    }
}

struct Node {
    next: Option<&'static mut Node>,
}

const BLOCK_SIZES: &[usize] = &[
    0x8, 0x10, 0x20, 0x40, 0x80, 0x100, 0x200, 0x400, 0x800, 0x1000,
];

unsafe impl GlobalAlloc for SpinLocked<BlockAlloc> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.lock().alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { self.lock().dealloc(NonNull::new_unchecked(ptr), layout) }
    }
}
