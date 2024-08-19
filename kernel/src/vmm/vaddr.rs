use core::{mem::MaybeUninit, ops::Range};

use shared::sys::SysError;

use super::{page_number, page_offset, Page, PageTable, PhysAddr, Pte, PteLink, SV39_LEVELS};

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

    /// Translate the virtual address `self` to a physical address through page table `pt`. Fails if
    /// no leaf PTE was found before `SV39_LEVELS` jumps or the leaf PTE permissions are missing any
    /// bits from `perms`.
    pub fn to_phys(self, mut pt: &PageTable, perms: Pte) -> Result<PhysAddr, VirtToPhysErr> {
        for level in (0..SV39_LEVELS).rev() {
            let entry = pt.0[self.vpn(level)];
            match entry.next() {
                PteLink::PageTable(next) => pt = unsafe { &*next },
                PteLink::Leaf(addr) if entry.perms().contains(perms) => {
                    return Ok(PhysAddr(addr as usize + self.offset(level)));
                }
                _ => break,
            }
        }
        Err(VirtToPhysErr)
    }

    /// Copy all of `buf` into address `self` in page table `pt`. Fails if any pages are not
    /// writable or accessible from user mode. May fail after a partial write.
    pub fn copy_to(
        self,
        pt: &PageTable,
        mut buf: &[u8],
        perms: Option<Pte>,
    ) -> Result<(), VirtToPhysErr> {
        for phys in self.iter_phys(pt, buf.len(), perms.unwrap_or(Pte::U | Pte::W)) {
            let phys = phys?;
            unsafe {
                let len = phys.end.sub_ptr(phys.start);
                core::ptr::copy_nonoverlapping(buf.as_ptr(), phys.start, len);
                buf = &buf[len..];
            }
        }

        Ok(())
    }

    /// Copy `buf.len()` bytes from address `self` in page table `pt`. Fails if any pages are not
    /// readable or accessible from user mode. May fail after a partial write.
    pub fn copy_from(
        self,
        pt: &PageTable,
        mut buf: &mut [MaybeUninit<u8>],
    ) -> Result<(), VirtToPhysErr> {
        for phys in self.iter_phys(pt, buf.len(), Pte::U | Pte::R) {
            let phys = phys?;
            unsafe {
                let len = phys.end.sub_ptr(phys.start);
                core::ptr::copy_nonoverlapping(phys.start, buf.as_mut_ptr().cast(), len);
                buf = &mut buf[len..];
            }
        }

        Ok(())
    }

    pub fn copy_type_from<T: Copy>(self, pt: &PageTable) -> Result<T, VirtToPhysErr> {
        // check align?
        let mut buf = MaybeUninit::<T>::uninit();
        self.copy_from(pt, buf.as_bytes_mut())?;
        Ok(unsafe { buf.assume_init() })
    }

    pub fn copy_type_to<T: Copy>(
        self,
        pt: &PageTable,
        buf: &T,
        perms: Option<Pte>,
    ) -> Result<(), VirtToPhysErr> {
        // check align?
        let s = unsafe {
            core::slice::from_raw_parts(buf as *const T as *const u8, core::mem::size_of::<T>())
        };
        self.copy_to(pt, s, perms)
    }

    pub fn iter_phys(self, pt: &PageTable, size: usize, perms: Pte) -> PhysIter {
        PhysIter {
            va: self,
            size,
            pt,
            perms,
        }
    }

    pub fn next_page(self) -> VirtAddr {
        VirtAddr(page_number(self.0 + Page::SIZE))
    }

    pub fn page(self) -> VirtAddr {
        VirtAddr(page_number(self.0))
    }

    pub(super) const fn vpn(self, level: usize) -> usize {
        (self.0 >> (12 + level * 9)) & 0x1ff
    }

    pub(super) const fn offset(self, level: usize) -> usize {
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

impl core::ops::BitAnd<usize> for VirtAddr {
    type Output = VirtAddr;

    fn bitand(self, rhs: usize) -> Self::Output {
        VirtAddr(self.0 & rhs)
    }
}

impl core::ops::BitOr<usize> for VirtAddr {
    type Output = VirtAddr;

    fn bitor(self, rhs: usize) -> Self::Output {
        VirtAddr(self.0 | rhs)
    }
}

impl core::ops::BitXor<usize> for VirtAddr {
    type Output = VirtAddr;

    fn bitxor(self, rhs: usize) -> Self::Output {
        VirtAddr(self.0 ^ rhs)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtToPhysErr;

impl From<VirtToPhysErr> for SysError {
    fn from(_: VirtToPhysErr) -> Self {
        Self::BadAddr
    }
}

pub struct PhysIter<'a> {
    va: VirtAddr,
    pt: &'a PageTable,
    size: usize,
    perms: Pte,
}

impl PhysIter<'_> {
    pub fn zero(self) {
        for page in self {
            unsafe {
                core::slice::from_mut_ptr_range(page.unwrap()).fill(0);
            }
        }
    }
}

impl Iterator for PhysIter<'_> {
    type Item = Result<Range<*mut u8>, VirtToPhysErr>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.size == 0 {
            return None;
        }

        // TODO: mega/gigapage
        let phys = match self.va.to_phys(self.pt, self.perms) {
            Ok(phys) => phys,
            Err(err) => return Some(Err(err)),
        };
        let size = (Page::SIZE - page_offset(self.va.0)).min(self.size);

        self.va.0 = page_number(self.va.0 + Page::SIZE);
        self.size -= size;

        Some(Ok(Range {
            start: phys.0 as *mut u8,
            end: (phys.0 + size) as *mut u8,
        }))
    }
}
