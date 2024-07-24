/*
Sv39 Physical Address

60 - 63 | Mode
44 - 59 | Address Space ID (ASID)
 0 - 43 | Physical Page Number (PPN)

Sv39 Virtual Address

30 - 38 | Virtual Page Number 2 (VPN)
21 - 29 | VPN1
12 - 20 | VPN0
 0 - 11 | Page Offset

*/

use core::ptr::NonNull;

use alloc::boxed::Box;
use either::Either;

pub const SATP_MODE_SV39: u64 = 8;

pub const PTE_V: u64 = 1 << 0;
pub const PTE_R: u64 = 1 << 1;
pub const PTE_W: u64 = 1 << 2;
pub const PTE_X: u64 = 1 << 3;
pub const PTE_U: u64 = 1 << 4;
pub const PTE_G: u64 = 1 << 5;
pub const PTE_A: u64 = 1 << 6;
pub const PTE_D: u64 = 1 << 7;
pub const PTE_RSW: u64 = (1 << 8) | (1 << 9);

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

#[repr(align(0x1000))]
pub struct PageTable([PageTableEntry; 512]);

impl PageTable {
    pub const fn new() -> Self {
        PageTable([PageTableEntry(0); 512])
    }

    pub fn map(&mut self, va: VirtAddr, pa: PhysAddr, perms: u64) -> bool {
        // crate::println!(
        //     "   Mapping pa {:?} to va {:?}",
        //     pa.0 as *const u8,
        //     va.0 as *const u8,
        // );

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

    pub fn map_range(&mut self, mut va: VirtAddr, pa: PhysAddr, size: usize, perms: u64) -> bool {
        if size == 0 {
            return true;
        }

        va.0 &= !0xfff;
        for page in (0..size + 0xfff).step_by(0x1000) {
            if !self.map(VirtAddr(va.0 + page), PhysAddr(pa.0 + page), perms) {
                return false;
            }
        }

        true
    }

    pub fn map_identity(&mut self, pa: PhysAddr, size: usize, perms: u64) -> bool {
        crate::println!(
            "Identity mapping region {:?} with size {size:#x}",
            pa.0 as *const u8
        );
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
