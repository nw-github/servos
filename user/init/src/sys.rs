use core::{convert::Infallible, mem::MaybeUninit};

use shared::{
    io::{DirEntry, OpenFlags},
    sys::{Sys, SysError},
};

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawFd(pub usize);

pub fn syscall(no: Sys, a0: usize, a1: usize, a2: usize, a3: usize) -> Result<usize, SysError> {
    let (result, err): (usize, usize);
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") no as usize,
            in("a0") a0,
            in("a1") a1,
            in("a2") a2,
            in("a3") a3,
            lateout("a0") result,
            lateout("a1") err,
        );
    }
    if err != 0 {
        Err(SysError::from_repr(err).unwrap())
    } else {
        Ok(result)
    }
}

pub fn shutdown(restart: bool) -> Result<Infallible, SysError> {
    Err(syscall(Sys::Shutdown, restart as usize, 0, 0, 0).unwrap_err())
}

pub fn close(fd: RawFd) -> Result<(), SysError> {
    syscall(Sys::Close, fd.0, 0, 0, 0).map(|_| ())
}

pub fn kill(pid: usize) -> Result<(), SysError> {
    syscall(Sys::Kill, pid, 0, 0, 0).map(|_| ())
}

pub fn getpid() -> usize {
    syscall(Sys::GetPid, 0, 0, 0, 0).unwrap() as usize
}

pub fn open(path: impl AsRef<[u8]>, flags: OpenFlags) -> Result<RawFd, SysError> {
    let path = path.as_ref();
    syscall(
        Sys::Open,
        path.as_ptr() as usize,
        path.len(),
        flags.bits() as usize,
        0,
    )
    .map(RawFd)
}

pub fn read(fd: RawFd, pos: u64, buf: &mut [u8]) -> Result<usize, SysError> {
    syscall(
        Sys::Read,
        fd.0,
        pos as usize,
        buf.as_mut_ptr() as usize,
        buf.len(),
    )
}

pub fn write(fd: RawFd, pos: u64, buf: &[u8]) -> Result<usize, SysError> {
    syscall(
        Sys::Write,
        fd.0,
        pos as usize,
        buf.as_ptr() as usize,
        buf.len(),
    )
}

pub fn readdir(fd: RawFd, pos: usize) -> Result<Option<DirEntry>, SysError> {
    let mut entry = MaybeUninit::<DirEntry>::uninit();
    if syscall(Sys::Readdir, fd.0, pos, entry.as_mut_ptr() as usize, 0)? == 0 {
        Ok(None)
    } else {
        Ok(Some(unsafe { entry.assume_init() }))
    }
}

pub fn chdir(path: impl AsRef<[u8]>) -> Result<(), SysError> {
    let path = path.as_ref();
    syscall(Sys::Chdir, path.as_ptr() as usize, path.len(), 0, 0).map(|_| ())
}
