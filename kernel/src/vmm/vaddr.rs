use core::{marker::PhantomData, mem::MaybeUninit, ops::Range};

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
                let len = phys.end.offset_from_unsigned(phys.start);
                core::ptr::copy_nonoverlapping(buf.as_ptr(), phys.start, len);
                buf = &buf[len..];
            }
        }

        Ok(())
    }

    /// Copy `buf.len()` bytes from address `self` in page table `pt`. Fails if any pages are not
    /// readable or accessible from user mode. May fail after a partial write.
    pub fn copy_from<'a>(
        self,
        pt: &PageTable,
        mut buf: &'a mut [MaybeUninit<u8>],
    ) -> Result<&'a mut [u8], VirtToPhysErr> {
        for phys in self.iter_phys(pt, buf.len(), Pte::U | Pte::R) {
            let phys = phys?;
            unsafe {
                let len = phys.end.offset_from_unsigned(phys.start);
                core::ptr::copy_nonoverlapping(phys.start, buf.as_mut_ptr().cast(), len);
                buf = &mut buf[len..];
            }
        }

        Ok(unsafe { buf.assume_init_mut() })
    }

    pub const fn iter_phys(self, pt: &PageTable, size: usize, perms: Pte) -> PhysIter {
        PhysIter {
            va: self,
            size,
            pt,
            perms,
        }
    }

    pub const fn next_page(self) -> VirtAddr {
        VirtAddr(page_number(self.0 + Page::SIZE))
    }

    pub const fn page(self) -> VirtAddr {
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

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct User<T>(VirtAddr, PhantomData<*const T>);

impl<T: Copy> User<T> {
    pub const fn new(addr: VirtAddr) -> User<T> {
        Self(addr, PhantomData)
    }

    pub fn read(self, pt: &PageTable) -> Result<T, VirtToPhysErr> {
        // check align?
        let mut buf = MaybeUninit::<T>::uninit();
        self.0.copy_from(pt, buf.as_bytes_mut())?;
        Ok(unsafe { buf.assume_init() })
    }

    pub fn read_arr<'a>(
        self,
        pt: &PageTable,
        buf: &'a mut [MaybeUninit<T>],
    ) -> Result<&'a mut [T], VirtToPhysErr> {
        self.0.copy_from(pt, buf.as_bytes_mut())?;
        Ok(unsafe { buf.assume_init_mut() })
    }

    pub fn write(self, pt: &PageTable, val: &T) -> Result<(), VirtToPhysErr> {
        // check align?
        self.0.copy_to(pt, as_byte_slice(val), None)
    }

    pub fn write_arr(
        self,
        pt: &PageTable,
        buf: &[T],
        perms: Option<Pte>,
    ) -> Result<(), VirtToPhysErr> {
        self.0.copy_to(pt, as_byte_slice(buf), perms)
    }

    pub const fn add(self, v: usize) -> User<T> {
        User(
            VirtAddr(self.addr().0 + v * core::mem::size_of::<T>()),
            PhantomData,
        )
    }

    pub const fn byte_add(self, v: usize) -> User<T> {
        User(VirtAddr(self.addr().0 + v), PhantomData)
    }

    pub const fn sub(self, v: usize) -> User<T> {
        User(
            VirtAddr(self.addr().0 - v * core::mem::size_of::<T>()),
            PhantomData,
        )
    }

    pub const fn byte_sub(self, v: usize) -> User<T> {
        User(VirtAddr(self.addr().0 - v), PhantomData)
    }

    pub const fn addr(self) -> VirtAddr {
        self.0
    }
}

impl<T: Copy> From<VirtAddr> for User<T> {
    fn from(value: VirtAddr) -> Self {
        Self::new(value)
    }
}

impl<T: Copy> From<usize> for User<T> {
    fn from(value: usize) -> Self {
        Self(VirtAddr(value), PhantomData)
    }
}

impl<T> From<User<T>> for VirtAddr {
    fn from(value: User<T>) -> Self {
        value.0
    }
}

fn as_byte_slice<T: ?Sized>(v: &T) -> &[u8] {
    unsafe { core::slice::from_raw_parts(core::ptr::from_ref(v).cast(), core::mem::size_of_val(v)) }
}
