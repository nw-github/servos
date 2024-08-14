use core::convert::Infallible;

use shared::{io::OpenFlags, sys::{Sys, SysError}};

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

pub fn close(fd: usize) -> Result<(), SysError> {
    syscall(Sys::Close, fd, 0, 0, 0).map(|_| ())
}

pub fn kill(pid: usize) -> Result<(), SysError> {
    syscall(Sys::Kill, pid, 0, 0, 0).map(|_| ())
}

pub fn getpid() -> usize {
    syscall(Sys::GetPid, 0, 0, 0, 0).unwrap() as usize
}

pub fn open(path: impl AsRef<[u8]>, flags: OpenFlags) -> Result<usize, SysError> {
    let path = path.as_ref();
    syscall(Sys::Open, path.as_ptr() as usize, path.len(), flags.bits() as usize, 0)
}

pub fn read(fd: usize, pos: u64, buf: &mut [u8]) -> Result<usize, SysError> {
    syscall(Sys::Read, fd, pos as usize, buf.as_mut_ptr() as usize, buf.len())
}

pub fn write(fd: usize, pos: u64, buf: &[u8]) -> Result<usize, SysError> {
    syscall(Sys::Write, fd, pos as usize, buf.as_ptr() as usize, buf.len())
}
