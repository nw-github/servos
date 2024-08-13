use core::{mem::MaybeUninit, ptr::NonNull};

use alloc::boxed::Box;

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

        /// Marks the page as owned by the page table, allowing the PageTable destructor to free it.
        /// This is potentially dangerous, as it will be automatically freed as a *mut Page when the
        /// PageTable destructor runs
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

pub const PGSIZE: usize = 0x1000;

#[derive(Debug)]
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

    /// Map a page that will be freed when the page table is dropped
    pub fn map_owned_page(&mut self, pa: Box<Page>, va: VirtAddr, perms: Pte) -> bool {
        assert!(perms.intersects(Pte::Rwx));
        assert!(va < VirtAddr::MAX);
        self.map_page_raw(Box::into_raw(pa).into(), va, perms | Pte::Owned)
    }

    /// Map the physical page containing `pa` to the virtual page `va` with permissions `perms`.
    ///
    /// Returns `false` if a required page table allocation fails when creating the mapping.
    ///
    /// # Panics
    ///
    /// Panics if the virtual page is already mapped or the virtual address is too large.
    pub fn map_page(&mut self, pa: PhysAddr, va: VirtAddr, perms: Pte) -> bool {
        assert!(perms.intersects(Pte::Rwx));
        assert!(va < VirtAddr::MAX);
        self.map_page_raw(pa, va, perms & !Pte::Owned)
    }

    /// Map all pages in the contiguous physical range `pa` to `pa + size` to the contiguous virtual
    /// range `va` to `va + size`. `pa` and `va` needn't be page aligned.
    pub fn map_pages(&mut self, pa: PhysAddr, mut va: VirtAddr, size: usize, perms: Pte) -> bool {
        assert!(perms.intersects(Pte::Rwx));
        assert!(size != 0);
        va.0 &= !(PGSIZE - 1);

        let [first, last] = [page_number(pa.0), page_number(pa.0.wrapping_add(size) - 1)];
        assert!(first < VirtAddr::MAX.0 && last < VirtAddr::MAX.0);
        for (i, page) in (first..=last).step_by(PGSIZE).enumerate() {
            if !self.map_page_raw(PhysAddr(page), va + i * PGSIZE, perms) {
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
            PGSIZE
        } else {
            end.0 - start.0
        };
        self.map_pages(start, VirtAddr(start.0), size, perms)
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
                    let Some(next) = Self::try_alloc().map(Box::into_raw) else {
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
    pub const MAX: VirtAddr = VirtAddr(1 << (12 + SV39_LEVELS * 9 - 1));

    /// Translate the virtual address `self` to a physical address through page table `pt`. Returns
    /// `None` if no leaf PTE was found before `SV39_LEVELS` jumps or the leaf PTE permissions are
    /// missing any bits from `perms`.
    pub fn to_phys(self, mut pt: &PageTable, perms: Pte) -> Option<PhysAddr> {
        for level in (0..SV39_LEVELS).rev() {
            let entry = pt.0[self.vpn(level)];
            match entry.next() {
                PteLink::PageTable(next) => pt = unsafe { &*next },
                PteLink::Leaf(addr) if entry.perms().contains(perms) => {
                    return Some(PhysAddr(addr as usize + self.offset(level)));
                }
                _ => return None,
            }
        }
        None
    }

    /// Copy all of `buf` into address `self` in page table `pt`. Fails if any pages are not
    /// writable or accessible from user mode. May fail after a partial write.
    pub fn ucopy_to(mut self, pt: &PageTable, mut buf: &[u8], perms: Option<Pte>) -> bool {
        let perms = perms.unwrap_or(Pte::U | Pte::W);
        while !buf.is_empty() {
            let Some(phys) = self.to_phys(pt, perms) else {
                return false;
            };

            let count = (PGSIZE - page_offset(self.0)).min(buf.len());
            unsafe {
                core::ptr::copy_nonoverlapping(buf.as_ptr(), phys.0 as *mut u8, count);
            }

            self.0 = page_number(self.0 + PGSIZE);
            buf = &buf[count..];
        }

        true
    }

    /// Copy `buf.len()` bytes from address `self` in page table `pt`. Fails if any pages are not
    /// readable or accessible from user mode. May fail after a partial write.
    pub fn ucopy_from(mut self, pt: &PageTable, mut buf: &mut [MaybeUninit<u8>]) -> bool {
        while !buf.is_empty() {
            let Some(phys) = self.to_phys(pt, Pte::U | Pte::R) else {
                return false;
            };

            let count = (PGSIZE - page_offset(self.0)).min(buf.len());
            unsafe {
                core::ptr::copy_nonoverlapping(phys.0 as *const u8, buf.as_mut_ptr().cast(), count);
            }

            self.0 = page_number(self.0 + PGSIZE);
            buf = &mut buf[count..];
        }

        true
    }

    const fn vpn(self, level: usize) -> usize {
        (self.0 >> (12 + level * 9)) & 0x1ff
    }

    const fn offset(self, level: usize) -> usize {
        self.0 & ((1 << (12 + level * 9)) - 1)
    }
}

impl core::fmt::Display for VirtAddr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:?}", self.0 as *const u8)
    }
}

impl core::ops::Add<usize> for VirtAddr {
    type Output = VirtAddr;

    fn add(self, rhs: usize) -> Self::Output {
        VirtAddr(self.0 + rhs)
    }
}

impl core::ops::Sub<usize> for VirtAddr {
    type Output = VirtAddr;

    fn sub(self, rhs: usize) -> Self::Output {
        VirtAddr(self.0 - rhs)
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
    pub fn zeroed() -> Option<Box<Page>> {
        Box::try_new_zeroed()
            .map(|page| unsafe { page.assume_init() })
            .ok()
    }

    pub fn uninit() -> Option<Box<Page>> {
        Box::try_new_uninit()
            .map(|page| unsafe { page.assume_init() })
            .ok()
    }

    pub unsafe fn cast<T>(&mut self) -> &mut T {
        debug_assert!(align_of::<T>() <= align_of::<Self>());
        unsafe { &mut *self.0.as_mut_ptr().cast() }
    }
}

#[inline(always)]
pub const fn page_number(addr: usize) -> usize {
    addr & !(PGSIZE - 1)
}

#[inline(always)]
pub const fn page_offset(addr: usize) -> usize {
    addr & (PGSIZE - 1)
}
