use core::{alloc::AllocError, mem::MaybeUninit};

use alloc::boxed::Box;

use super::{PhysAddr, VirtAddr};

#[repr(C, align(0x1000))]
pub struct Page(pub [MaybeUninit<u8>; Page::SIZE]);

impl Page {
    pub const SIZE: usize = 0x1000;

    pub fn zeroed() -> Result<Box<Page>, AllocError> {
        Box::try_new_zeroed().map(|page| unsafe { page.assume_init() })
    }

    pub fn uninit() -> Result<Box<Page>, AllocError> {
        Box::try_new_uninit().map(|page| unsafe { page.assume_init() })
    }
}

#[inline(always)]
pub const fn page_number(addr: usize) -> usize {
    addr & !(Page::SIZE - 1)
}

#[inline(always)]
pub const fn page_offset(addr: usize) -> usize {
    addr & (Page::SIZE - 1)
}

/// SATP register
///
/// 60 - 63 | Mode
///
/// 44 - 59 | Address Space ID (ASID)
///
///  0 - 43 | Physical Page Number (PPN)

pub const SATP_MODE_SV39: u64 = 8;
pub const SV39_LEVELS: usize = 3;

bitflags::bitflags! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct Pte: u64 {
        const V = 1 << 0;
        const R = 1 << 1;
        const W = 1 << 2;
        const X = 1 << 3;
        const U = 1 << 4;
        const G = 1 << 5;
        const A = 1 << 6;
        const D = 1 << 7;

        /// Marks the page as owned by the page table, meaning it will be freed with the PageTable.
        /// Pages marked with this bit must have been allocated with the layout of a Page.
        const Owned = 1 << 8;
        const Rsw1 = 1 << 9;

        const Rw  = (1 << 1) | (1 << 2);
        const Rx  = (1 << 1) | (1 << 3);
        const Rwx = (1 << 1) | (1 << 2) | (1 << 3);

        const Urw  = (1 << 4) | (1 << 1) | (1 << 2);
        const Urx  = (1 << 4) | (1 << 1) | (1 << 3);
        const Urwx = (1 << 4) | (1 << 1) | (1 << 2) | (1 << 3);
    }
}

#[derive(Debug)]
pub enum PteLink {
    Leaf(*mut u8),
    PageTable(*mut PageTable),
    Invalid,
}

#[repr(transparent)]
#[derive(Clone, Copy, Default)]
pub struct PageTableEntry(u64);

#[allow(unused)]
impl PageTableEntry {
    const fn new(pa: PhysAddr, perms: u64) -> Self {
        Self(((pa.0 as u64 >> 12) << 10) | (perms & 0x3ff) | Pte::V.bits())
    }

    pub const fn is_valid(self) -> bool {
        self.0 & Pte::V.bits() != 0
    }

    pub const fn is_read(self) -> bool {
        self.0 & Pte::R.bits() != 0
    }

    pub const fn is_write(self) -> bool {
        self.0 & Pte::W.bits() != 0
    }

    pub const fn is_execute(self) -> bool {
        self.0 & Pte::X.bits() != 0
    }

    pub const fn is_umode(self) -> bool {
        self.0 & Pte::U.bits() != 0
    }

    pub const fn is_global(self) -> bool {
        self.0 & Pte::G.bits() != 0
    }

    pub const fn is_accessed(self) -> bool {
        self.0 & Pte::A.bits() != 0
    }

    pub const fn is_dirty(self) -> bool {
        self.0 & Pte::D.bits() != 0
    }

    /// Is the physical page owned by this page table
    pub const fn is_owned(self) -> bool {
        self.0 & Pte::Owned.bits() != 0
    }

    pub const fn rsw1(self) -> bool {
        self.0 & Pte::Rsw1.bits() != 0
    }

    pub const fn is_leaf(self) -> bool {
        self.0 & Pte::Rwx.bits() != 0
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

    pub const fn perms(self) -> Pte {
        Pte::from_bits_retain(self.0 & 0x3ff)
    }
}

#[repr(C, align(0x1000))] // Page::SIZE
pub struct PageTable(pub(super) [PageTableEntry; 512]);

impl PageTable {
    pub const fn new() -> Self {
        PageTable([PageTableEntry(0); 512])
    }

    pub fn alloc() -> Result<Box<PageTable>, AllocError> {
        Box::<PageTable>::try_new_zeroed().map(|ptr| unsafe { ptr.assume_init() })
    }

    /// Map a page that will be freed when the page table is dropped
    pub fn map_owned_page(&mut self, pa: Box<Page>, va: VirtAddr, perms: Pte) -> bool {
        assert!(perms.intersects(Pte::Rwx));
        assert!(va < VirtAddr::MAX);
        self.map_page_raw(Box::into_raw(pa).into(), va, perms | Pte::Owned)
    }

    /// Map all pages in the contiguous physical range `pa` to `pa + size` to the contiguous virtual
    /// range `va` to `va + size`. `pa` and `va` needn't be page aligned.
    pub fn map_pages(&mut self, pa: PhysAddr, mut va: VirtAddr, size: usize, perms: Pte) -> bool {
        assert!(perms.intersects(Pte::Rwx));
        assert!(size != 0);
        va.0 = page_number(va.0);

        let [first, last] = [page_number(pa.0), page_number(pa.0.wrapping_add(size) - 1)];
        assert!(first < VirtAddr::MAX.0 && last < VirtAddr::MAX.0 && first <= last);
        for (i, page) in (first..=last).step_by(Page::SIZE).enumerate() {
            if !self.map_page_raw(PhysAddr(page), va + i * Page::SIZE, perms & !Pte::Owned) {
                return false;
            }
        }

        true
    }

    pub fn map_new_pages(&mut self, va: VirtAddr, size: usize, perms: Pte, zero: bool) -> bool {
        assert!(perms.intersects(Pte::Rwx));
        assert!(size != 0);

        let [first, last] = [page_number(va.0), page_number(va.0.wrapping_add(size) - 1)];
        if !(first < VirtAddr::MAX.0 && last < VirtAddr::MAX.0 && first <= last) {
            return false;
        }
        for page in (first..=last).step_by(Page::SIZE) {
            let Ok(pa) = (if zero { Page::zeroed() } else { Page::uninit() }) else {
                return false;
            };
            if !self.map_page_raw(Box::into_raw(pa).into(), VirtAddr(page), perms | Pte::Owned) {
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
        perms: Pte,
    ) -> bool {
        let (start, end) = (start.into(), end.into());
        let size = if start == end {
            Page::SIZE
        } else {
            end.0 - start.0
        };
        self.map_pages(start, VirtAddr(start.0), size, perms)
    }

    pub fn unmap_pages(&mut self, va: VirtAddr, va_end: VirtAddr) {
        assert!(va < VirtAddr::MAX && va_end < VirtAddr::MAX && va <= va_end);
        for page in (va.page().0..=va_end.page().0).step_by(Page::SIZE) {
            self.unmap_page(VirtAddr(page));
        }
    }

    pub fn unmap_page(&mut self, va: VirtAddr) -> bool {
        let mut pt = self;
        for level in (0..SV39_LEVELS).rev() {
            let entry = &mut pt.0[va.vpn(level)];
            match entry.next() {
                PteLink::PageTable(next) => pt = unsafe { &mut *next },
                PteLink::Leaf(page) => {
                    assert!(level == 0, "Page table level {level} is a leaf node");
                    if entry.is_owned() {
                        drop(unsafe { Box::from_raw(page as *mut Page) });
                    }
                    *entry = PageTableEntry(0);
                    return true;
                }
                PteLink::Invalid => break,
            }
        }

        false
    }

    pub fn make_satp(this: *const PageTable) -> usize {
        ((SATP_MODE_SV39 as usize) << 60) | (this as usize >> 12)
    }

    fn map_page_raw(&mut self, pa: PhysAddr, va: VirtAddr, perms: Pte) -> bool {
        let mut pt = self;
        for level in (1..SV39_LEVELS).rev() {
            let entry = &mut pt.0[va.vpn(level)];
            match entry.next() {
                PteLink::PageTable(next) => pt = unsafe { &mut *next },
                PteLink::Leaf(_) => panic!("Page table {level} is a leaf node"),
                PteLink::Invalid => {
                    let Ok(next) = Self::alloc().map(Box::into_raw) else {
                        return false;
                    };
                    *entry = PageTableEntry::new(next.into(), 0);
                    pt = unsafe { &mut *next };
                }
            }
        }

        let entry = &mut pt.0[va.vpn(0)];
        assert!(
            matches!(entry.next(), PteLink::Invalid),
            "remapping virtual addr (was {:?})",
            entry.next(),
        );
        // the A and D bits can be treated as secondary R and W bits on some boards
        *entry = PageTableEntry::new(pa, (perms | Pte::D | Pte::A).bits());
        true
    }
}

impl Drop for PageTable {
    fn drop(&mut self) {
        for &entry in self.0.iter() {
            match entry.next() {
                PteLink::PageTable(pt) => drop(unsafe { Box::from_raw(pt) }),
                PteLink::Leaf(page) if entry.is_owned() => {
                    drop(unsafe { Box::from_raw(page as *mut Page) });
                }
                _ => {}
            }
        }
    }
}
