use core::ptr::NonNull;

use alloc::boxed::Box;

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
pub const PTE_RSW0: u64 = 1 << 8;
pub const PTE_RSW1: u64 = 1 << 9;

pub const PGSIZE: usize = 0x1000;

pub enum PteLink {
    Leaf(*mut u8),
    PageTable(*mut PageTable),
    Invalid,
}

#[repr(transparent)]
#[derive(Clone, Copy, Default)]
pub struct PageTableEntry(u64);

impl PageTableEntry {
    fn new(pa: PhysAddr, perms: u64) -> Self {
        // the A and D bits can be treated as secondary R and W bits on some boards
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

    pub const fn rsw0(self) -> bool {
        self.0 & PTE_RSW0 != 0
    }

    pub const fn rsw1(self) -> bool {
        self.0 & PTE_RSW1 != 0
    }

    pub const fn is_leaf(self) -> bool {
        self.0 & (PTE_R | PTE_W | PTE_X) != 0
    }

    pub const fn next(self) -> PteLink {
        let addr = (self.0 >> 10) << 12;
        if !self.is_valid() {
            PteLink::Invalid
        } else if self.is_leaf() {
            PteLink::Leaf(addr as *mut _)
        } else {
            PteLink::PageTable(addr as *mut _)
        }
    }
}

#[repr(C, align(0x1000))] // PGSIZE
pub struct PageTable([PageTableEntry; 512]);

impl PageTable {
    pub const fn new() -> Self {
        PageTable([PageTableEntry(0); 512])
    }

    pub fn try_alloc() -> Option<Box<PageTable>> {
        Box::<PageTable>::try_new_zeroed()
            .map(|ptr| unsafe { ptr.assume_init() })
            .ok()
    }

    /// Map the physical page containing `pa` to the virtual page `va` with permissions `perms`.
    ///
    /// Returns `false` if a required page table allocation fails when creating the mapping.
    ///
    /// # Panics
    ///
    /// If the virtual page is already mapped, any pte except the final one is a leaf entry, or the
    /// virtual address is too large, this function will panic.
    pub fn map_page(&mut self, pa: PhysAddr, va: VirtAddr, perms: u64) -> bool {
        assert!(va < VirtAddr::MAX);
        assert!(perms & (PTE_R | PTE_W | PTE_X) != 0);

        let mut pt = self;
        for level in (1..=2).rev() {
            let entry = &mut pt.0[va.vpn(level)];
            match entry.next() {
                PteLink::PageTable(next) => pt = unsafe { &mut *next },
                PteLink::Leaf(_) => panic!("Page table {level} is a leaf node"),
                PteLink::Invalid => {
                    let Some(next) = Self::try_alloc().map(Box::into_raw) else {
                        return false;
                    };
                    *entry = PageTableEntry::new(next.into(), 0);
                    pt = unsafe { &mut *next };
                }
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
    pub fn map_range(&mut self, pa: PhysAddr, mut va: VirtAddr, size: usize, perms: u64) -> bool {
        assert!(size != 0);
        va.0 &= !(PGSIZE - 1);

        let [first, last] = [page_number(pa), page_number(pa.0 + size - 1)];
        for (i, page) in (first..=last).step_by(PGSIZE).enumerate() {
            if !self.map_page(PhysAddr(page), VirtAddr(va.0 + i * PGSIZE), perms) {
                return false;
            }
        }

        true
    }

    /// Identity map all pages in the contigous physical range `pa` to `pa + size`. Only intended
    /// for use by the kernel.
    pub fn map_identity(
        &mut self,
        start: impl Into<PhysAddr>,
        end: impl Into<PhysAddr>,
        perms: u64,
    ) -> bool {
        let (start, end) = (start.into(), end.into());
        let size = if start == end {
            PGSIZE
        } else {
            end.0 - start.0
        };
        self.map_range(start, VirtAddr(start.0), size, perms)
    }

    pub fn make_satp(this: *const PageTable) -> usize {
        ((SATP_MODE_SV39 as usize) << 60) | (this as usize >> 12)
    }
}

impl Drop for PageTable {
    fn drop(&mut self) {
        for entry in self.0.into_iter().filter(|e| e.is_valid()) {
            if let PteLink::PageTable(pt) = entry.next() {
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
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VirtAddr(pub usize);

impl VirtAddr {
    #[allow(clippy::unusual_byte_groupings)]
    pub const MAX: VirtAddr = VirtAddr(0b100000000_000000000_000000000_000000000000);

    pub fn to_phys(self, mut pt: &PageTable) -> Option<PhysAddr> {
        for level in (0..=2).rev() {
            let entry = pt.0[self.vpn(level)];
            match entry.next() {
                PteLink::Leaf(addr) => {
                    let offset = (1 << (12 + level * 9)) - 1;
                    return Some(PhysAddr(addr as usize + (self.0 & offset)));
                }
                PteLink::PageTable(next) => pt = unsafe { &*next },
                PteLink::Invalid => return None,
            }
        }
        None
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

#[repr(C, align(0x1000))]
pub struct Page(pub [u8; PGSIZE]);

impl Page {
    pub fn alloc() -> Option<Box<Page>> {
        Box::try_new_zeroed()
            .map(|page| unsafe { page.assume_init() })
            .ok()
    }
}

pub fn page_number(addr: impl Into<PhysAddr>) -> usize {
    addr.into().0 & !(PGSIZE - 1)
}

pub fn page_offset(addr: impl Into<PhysAddr>) -> usize {
    addr.into().0 & (PGSIZE - 1)
}
