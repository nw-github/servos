use core::ptr::NonNull;

use alloc::boxed::Box;
use either::Either;

/// SATP register
///
/// 60 - 63 | Mode
///
/// 44 - 59 | Address Space ID (ASID)
///
///  0 - 43 | Physical Page Number (PPN)

pub const SATP_MODE_SV39: u64 = 8;

pub const PTE_V: u64 = 1 << 0;
pub const PTE_R: u64 = 1 << 1;
pub const PTE_W: u64 = 1 << 2;
pub const PTE_X: u64 = 1 << 3;
pub const PTE_U: u64 = 1 << 4;
pub const PTE_G: u64 = 1 << 5;
pub const PTE_A: u64 = 1 << 6;
pub const PTE_D: u64 = 1 << 7;
// pub const PTE_RSW: u64 = (1 << 8) | (1 << 9);

pub const PGSIZE: usize = 0x1000;

#[repr(transparent)]
#[derive(Clone, Copy, Default)]
pub struct PageTableEntry(u64);

impl PageTableEntry {
    fn new(pa: PhysAddr, perms: u64) -> Self {
        Self(((pa.0 as u64 >> 12) << 10) | (perms & 0x3ff) | PTE_V)
    }

    pub const fn is_valid(self) -> bool {
        self.0 & PTE_V != 0
    }

    pub const fn is_read(self) -> bool {
        self.0 & PTE_R != 0
    }

    pub const fn is_write(self) -> bool {
        self.0 & PTE_W != 0
    }

    pub const fn is_execute(self) -> bool {
        self.0 & PTE_X != 0
    }

    pub const fn is_umode(self) -> bool {
        self.0 & PTE_U != 0
    }

    pub const fn is_global(self) -> bool {
        self.0 & PTE_G != 0
    }

    pub const fn is_accessed(self) -> bool {
        self.0 & PTE_A != 0
    }

    pub const fn is_dirty(self) -> bool {
        self.0 & PTE_D != 0
    }

    pub const fn rsw(self) -> u64 {
        (self.0 >> 8) & 0b11
    }

    pub const fn is_leaf(self) -> bool {
        self.0 & 0b1110 != 0
    }

    pub const fn next(self) -> Either<*mut PageTable, *mut u8> {
        let addr = (self.0 >> 10) << 12;
        if self.is_leaf() {
            Either::Right(addr as *mut _)
        } else {
            Either::Left(addr as *mut _)
        }
    }
}

#[repr(align(0x1000))] // PGSIZE
pub struct PageTable([PageTableEntry; 512]);

impl PageTable {
    pub const fn new() -> Self {
        PageTable([PageTableEntry(0); 512])
    }

    /// Map the physical page containing `pa` to the virtual page `va` with permissions `perms`.
    ///
    /// Returns `false` if a required page table allocation fails when creating the mapping.
    ///
    /// # Panics
    ///
    /// If the virtual page is already mapped, or any pte except the final one is a leaf entry,
    /// this function will panic.
    pub fn map(&mut self, va: VirtAddr, pa: PhysAddr, perms: u64) -> bool {
        let mut pt = self;
        for level in [2, 1] {
            let entry = &mut pt.0[va.vpn(level)];
            if !entry.is_valid() {
                let Ok(ptr) = Box::<PageTable>::try_new_zeroed() else {
                    return false;
                };
                let ptr = Box::into_raw(unsafe { ptr.assume_init() });
                *entry = PageTableEntry::new(ptr.into(), 0);
                pt = unsafe { &mut *ptr };
            } else if let Either::Left(next) = entry.next() {
                pt = unsafe { &mut *next };
            } else {
                panic!("Page table {level} is a leaf node");
            }
        }

        let entry = &mut pt.0[va.vpn(0)];
        if entry.is_valid() {
            panic!("PageTable::map remap");
        }
        *entry = PageTableEntry::new(pa, perms);
        true
    }

    /// Map all pages in the contiguous physical range `pa` to `pa + size` to the contiguous virtual
    /// range `va` to `va + size`. `pa` and `va` needn't be page aligned.
    pub fn map_range(&mut self, mut va: VirtAddr, pa: PhysAddr, size: usize, perms: u64) -> bool {
        if size == 0 {
            return true;
        }
        va.0 &= !(PGSIZE - 1);

        let [first, last] = [pa.0 & !(PGSIZE - 1), (pa.0 + size - 1) & !(PGSIZE - 1)];
        for (i, page) in (first..=last).step_by(PGSIZE).enumerate() {
            if !self.map(VirtAddr(va.0 + i * PGSIZE), PhysAddr(page), perms) {
                return false;
            }
        }

        true
    }

    /// Identity map all pages in the contigous physical range `pa` to `pa + size`. Only intended
    /// for use by the kernel.
    pub fn map_identity(&mut self, pa: PhysAddr, size: usize, perms: u64) -> bool {
        self.map_range(VirtAddr(pa.0), pa, size, perms)
    }
}

impl Drop for PageTable {
    fn drop(&mut self) {
        for entry in self.0.into_iter().filter(|e| e.is_valid()) {
            if let Either::Left(pt) = entry.next() {
                drop(unsafe { Box::from_raw(pt) });
            }
        }
    }
}

/// Sv39 Virtual Address
///
/// 30 - 38 | Virtual Page Number (VPN) 2
///
/// 21 - 29 | VPN1
///
/// 12 - 20 | VPN0
///
///  0 - 11 | Page Offset
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct VirtAddr(pub usize);

impl VirtAddr {
    pub fn translate(self, mut pt: &PageTable) -> Option<PhysAddr> {
        let mut level = 2;
        loop {
            let entry = pt.0[self.vpn(level)];
            if !entry.is_valid() {
                return None;
            }

            match entry.next() {
                Either::Left(next) => {
                    assert!(level != 0, "Page table 0 is not a leaf node!");
                    pt = unsafe { &*next };
                }
                Either::Right(data) => {
                    assert!(level == 0, "Page table {level} is a leaf node");
                    return Some(PhysAddr(data as usize + self.offset()));
                }
            }
            level -= 1;
        }
    }

    pub fn offset(self) -> usize {
        self.0
    }

    pub const fn vpn(self, vpn: usize) -> usize {
        debug_assert!(vpn < 3);
        (self.0 >> (12 + vpn * 9)) & 0x1ff
    }
}

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

impl<T> From<NonNull<T>> for PhysAddr {
    fn from(value: NonNull<T>) -> Self {
        Self(value.as_ptr() as usize)
    }
}
