use core::{convert::Infallible, marker::PhantomData, mem::MaybeUninit};

pub use shared::sys::*;

use shared::io::{DirEntry, OpenFlags, Stat};

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawFd(pub usize);

macro_rules! syscall {
    ($no: expr $(,)?) => {
        {
            let no: Sys = $no;
            let (result, err): (usize, usize);
            unsafe {
                core::arch::asm!(
                    "ecall",
                    in("a7") no as usize,
                    lateout("a0") result,
                    lateout("a1") err,
                );
            }
            sys_result(result, err)
        }
    };
    ($no: expr, $a0: expr $(,)?) => {
        {
            let no: Sys = $no;
            let (result, err): (usize, usize);
            unsafe {
                core::arch::asm!(
                    "ecall",
                    in("a7") no as usize,
                    in("a0") $a0,
                    lateout("a0") result,
                    lateout("a1") err,
                );
            }
            sys_result(result, err)
        }
    };
    ($no: expr, $a0: expr, $a1: expr $(,)?) => {
        {
            let no: Sys = $no;
            let (result, err): (usize, usize);
            unsafe {
                core::arch::asm!(
                    "ecall",
                    in("a7") no as usize,
                    in("a0") $a0,
                    in("a1") $a1,
                    lateout("a0") result,
                    lateout("a1") err,
                );
            }
            sys_result(result, err)
        }
    };
    ($no: expr, $a0: expr, $a1: expr, $a2: expr $(,)?) => {
        {
            let no: Sys = $no;
            let (result, err): (usize, usize);
            unsafe {
                core::arch::asm!(
                    "ecall",
                    in("a7") no as usize,
                    in("a0") $a0,
                    in("a1") $a1,
                    in("a2") $a2,
                    lateout("a0") result,
                    lateout("a1") err,
                );
            }
            sys_result(result, err)
        }
    };
    ($no: expr, $a0: expr, $a1: expr, $a2: expr, $a3: expr $(,)?) => {
        {
            let no: Sys = $no;
            let (result, err): (usize, usize);
            unsafe {
                core::arch::asm!(
                    "ecall",
                    in("a7") no as usize,
                    in("a0") $a0,
                    in("a1") $a1,
                    in("a2") $a2,
                    in("a3") $a3,
                    lateout("a0") result,
                    lateout("a1") err,
                );
            }
            sys_result(result, err)
        }
    };
    ($no: expr, $a0: expr, $a1: expr, $a2: expr, $a3: expr, $a4: expr $(,)?) => {
        {
            let no: Sys = $no;
            let (result, err): (usize, usize);
            unsafe {
                core::arch::asm!(
                    "ecall",
                    in("a7") no as usize,
                    in("a0") $a0,
                    in("a1") $a1,
                    in("a2") $a2,
                    in("a3") $a3,
                    in("a4") $a4,
                    lateout("a0") result,
                    lateout("a1") err,
                );
            }
            sys_result(result, err)
        }
    };
    ($no: expr, $a0: expr, $a1: expr, $a2: expr, $a3: expr, $a4: expr, $a5: expr $(,)?) => {
        {
            let no: Sys = $no;
            let (result, err): (usize, usize);
            unsafe {
                core::arch::asm!(
                    "ecall",
                    in("a7") no as usize,
                    in("a0") $a0,
                    in("a1") $a1,
                    in("a2") $a2,
                    in("a3") $a3,
                    in("a4") $a4,
                    in("a5") $a5,
                    lateout("a0") result,
                    lateout("a1") err,
                );
            }
            sys_result(result, err)
        }
    };
    ($no: expr, $a0: expr, $a1: expr, $a2: expr, $a3: expr, $a4: expr, $a5: expr, $a6: expr $(,)?) => {
        {
            let no: Sys = $no;
            let (result, err): (usize, usize);
            unsafe {
                core::arch::asm!(
                    "ecall",
                    in("a7") no as usize,
                    in("a0") $a0,
                    in("a1") $a1,
                    in("a2") $a2,
                    in("a3") $a3,
                    in("a4") $a4,
                    in("a5") $a5,
                    in("a6") $a6,
                    lateout("a0") result,
                    lateout("a1") err,
                );
            }
            sys_result(result, err)
        }
    };
}

#[inline(always)]
fn sys_result(result: usize, err: usize) -> Result<usize, SysError> {
    if err != 0 {
        Err(SysError::from_repr(err).unwrap())
    } else {
        Ok(result)
    }
}

pub fn shutdown(restart: bool) -> Result<Infallible, SysError> {
    Err(syscall!(Sys::Shutdown, restart as usize).unwrap_err())
}

pub fn close(fd: RawFd) -> Result<(), SysError> {
    syscall!(Sys::Close, fd.0).map(|_| ())
}

pub fn kill(pid: u32) -> Result<(), SysError> {
    syscall!(Sys::Kill, pid as usize).map(|_| ())
}

pub fn getpid() -> u32 {
    syscall!(Sys::GetPid).unwrap() as u32
}

pub fn open(path: impl AsRef<[u8]>, flags: OpenFlags) -> Result<RawFd, SysError> {
    let path = path.as_ref();
    syscall!(
        Sys::Open,
        path.as_ptr() as usize,
        path.len(),
        flags.bits() as usize,
    )
    .map(RawFd)
}

pub fn read(fd: RawFd, pos: impl Into<Option<u64>>, buf: &mut [u8]) -> Result<usize, SysError> {
    syscall!(
        Sys::Read,
        fd.0,
        pos.into().unwrap_or(u64::MAX) as usize,
        buf.as_mut_ptr() as usize,
        buf.len(),
    )
}

pub fn write(fd: RawFd, pos: impl Into<Option<u64>>, buf: &[u8]) -> Result<usize, SysError> {
    syscall!(
        Sys::Write,
        fd.0,
        pos.into().unwrap_or(u64::MAX) as usize,
        buf.as_ptr() as usize,
        buf.len(),
    )
}

pub fn readdir(fd: RawFd, pos: impl Into<Option<usize>>) -> Result<Option<DirEntry>, SysError> {
    let mut entry = MaybeUninit::<DirEntry>::uninit();
    let res = syscall!(
        Sys::Readdir,
        fd.0,
        pos.into().unwrap_or(usize::MAX),
        entry.as_mut_ptr() as usize,
    );
    res.map(|res| (res != 0).then(|| unsafe { entry.assume_init() }))
}

pub fn stat(fd: RawFd) -> Result<Stat, SysError> {
    let mut entry = MaybeUninit::<Stat>::uninit();
    syscall!(Sys::Stat, fd.0, entry.as_mut_ptr() as usize)?;
    Ok(unsafe { entry.assume_init() })
}

pub fn chdir(path: impl AsRef<[u8]>) -> Result<(), SysError> {
    let path = path.as_ref();
    syscall!(Sys::Chdir, path.as_ptr() as usize, path.len()).map(|_| ())
}

pub fn sbrk(inc: isize) -> Result<*mut u8, SysError> {
    syscall!(Sys::Sbrk, inc as usize).map(|addr| addr as *mut u8)
}

pub fn spawn(path: impl AsRef<[u8]>, args: &[KString]) -> Result<u32, SysError> {
    let path = path.as_ref();
    syscall!(
        Sys::Spawn,
        path.as_ptr() as usize,
        path.len(),
        args.as_ptr() as usize,
        args.len(),
    )
    .map(|pid| pid as u32)
}

pub fn waitpid(pid: u32) -> Result<usize, SysError> {
    syscall!(Sys::Waitpid, pid as usize)
}

pub fn exit(ecode: usize) -> Result<Infallible, SysError> {
    Err(syscall!(Sys::Exit, ecode).unwrap_err())
}

pub fn debug(str: impl AsRef<[u8]>) -> Result<(), SysError> {
    let str = str.as_ref();
    syscall!(Sys::Debug, str.as_ptr() as usize, str.len())?;
    Ok(())
}

#[repr(C)]
pub struct KString<'a> {
    buf: *const u8,
    len: usize,
    _pd: PhantomData<&'a u8>,
}

impl<'a> KString<'a> {
    pub fn new(value: impl AsRef<[u8]>) -> Self {
        let value = value.as_ref();
        Self {
            buf: value.as_ptr(),
            len: value.len(),
            _pd: PhantomData,
        }
    }
}

impl<'a> From<&'a [u8]> for KString<'a> {
    fn from(value: &'a [u8]) -> Self {
        Self {
            buf: value.as_ptr(),
            len: value.len(),
            _pd: PhantomData,
        }
    }
}

impl<'a> From<&'a str> for KString<'a> {
    fn from(value: &'a str) -> Self {
        Self {
            buf: value.as_ptr(),
            len: value.len(),
            _pd: PhantomData,
        }
    }
}
