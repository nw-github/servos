use core::arch::asm;

// https://github.com/riscv-non-isa/riscv-sbi-doc/blob/master/src/binary-encoding.adoc

#[repr(isize)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SbiError {
    Failed = -1,
    NotSupported = -2,
    InvalidParam = -3,
    Denied = -4,
    InvalidAddress = -5,
    AlreadyAvailable = -6,
    AlreadyStarted = -7,
    AlreadyStopped = -8,
    NoShmem = -9,
    InvalidState = -10,
    BadRange = -11,
}

pub type SbiResult<T> = Result<T, SbiError>;

pub struct SbiRet {
    pub error: isize,
    pub value: isize,
}

impl SbiRet {
    pub unsafe fn from_raw(error: isize, value: isize) -> Self {
        debug_assert!(
            (-11..=0).contains(&error),
            "sbi error outside defined range"
        );
        SbiRet { error, value }
    }

    pub fn into_result<T>(self, f: impl FnOnce(isize) -> T) -> SbiResult<T> {
        if self.error == 0 {
            Ok(f(self.value))
        } else {
            Err(unsafe { core::mem::transmute::<isize, SbiError>(self.error) })
        }
    }
}

#[inline(always)]
pub fn sbicall_0(eid: i32, fid: i32) -> SbiRet {
    let (error, value);
    unsafe {
        asm!(
            "ecall",
            in("a7") eid,
            in("a6") fid,
            lateout("a0") error,
            lateout("a1") value,
        );
        SbiRet::from_raw(error, value)
    }
}

#[inline(always)]
pub fn sbicall_1(eid: i32, fid: i32, a0: usize) -> SbiRet {
    let (error, value);
    unsafe {
        asm!(
            "ecall",
            in("a7") eid,
            in("a6") fid,
            in("a0") a0,
            lateout("a0") error,
            lateout("a1") value,
        );
        SbiRet::from_raw(error, value)
    }
}

#[inline(always)]
pub fn sbicall_2(eid: i32, fid: i32, a0: usize, a1: usize) -> SbiRet {
    let (error, value);
    unsafe {
        asm!(
            "ecall",
            in("a7") eid,
            in("a6") fid,
            in("a0") a0,
            in("a1") a1,
            lateout("a0") error,
            lateout("a1") value,
        );
        SbiRet::from_raw(error, value)
    }
}

#[inline(always)]
pub fn sbicall_3(eid: i32, fid: i32, a0: usize, a1: usize, a2: usize) -> SbiRet {
    let (error, value);
    unsafe {
        asm!(
            "ecall",
            in("a7") eid,
            in("a6") fid,
            in("a0") a0,
            in("a1") a1,
            in("a2") a2,
            lateout("a0") error,
            lateout("a1") value,
        );
        SbiRet::from_raw(error, value)
    }
}
