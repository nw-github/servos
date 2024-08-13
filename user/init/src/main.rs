#![no_std]
#![no_main]

use core::convert::Infallible;

use shared::sys::{Sys, SysError};

#[panic_handler]
fn on_panic(_: &core::panic::PanicInfo) -> ! {
    kill(getpid());
    loop {}
}

fn syscall(no: Sys, a0: usize, a1: usize, a2: usize, a3: usize) -> Result<usize, SysError> {
    let result: isize;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") no as usize,
            in("a0") a0,
            in("a1") a1,
            in("a2") a2,
            in("a3") a3,
            lateout("a0") result,
        );
    }
    if result < 0 {
        Err(SysError::from_repr(-result).unwrap())
    } else {
        Ok(result as usize)
    }
}

fn shutdown(restart: bool) -> Result<Infallible, SysError> {
    Err(syscall(Sys::Shutdown, restart as usize, 0, 0, 0).unwrap_err())
}

fn close(fd: usize) -> Result<(), SysError> {
    syscall(Sys::Close, fd, 0, 0, 0).map(|_| ())
}

fn kill(pid: usize) -> Result<(), SysError> {
    syscall(Sys::Kill, pid, 0, 0, 0).map(|_| ())
}

fn getpid() -> usize {
    syscall(Sys::GetPid, 0, 0, 0, 0).unwrap() as usize
}

// uint open(const char *path, uint pathlen, u32 flags);
fn open(path: impl AsRef<[u8]>, flags: u32) -> Result<usize, SysError> {
    let path = path.as_ref();
    syscall(Sys::Open, path.as_ptr() as usize, path.len(), flags as usize, 0)
}

// uint read(uint fd, u64 pos, char *buf, uint buflen);
fn read(fd: usize, pos: u64, buf: &mut [u8]) -> Result<usize, SysError> {
    syscall(Sys::Read, fd, pos as usize, buf.as_mut_ptr() as usize, buf.len())
}

static mut GLOBAL_STATIC: usize = 5;

#[no_mangle]
pub extern "C" fn _start() {
    unsafe {
        while GLOBAL_STATIC != 0 {
            GLOBAL_STATIC -= 1;
        }
    }

    let mut buf = [0; 0x2000];
    let fd = open("/test.txt", 0).unwrap();
    let read = read(fd, 0, &mut buf).unwrap();

    let mut res = 0usize;
    for &b in buf[..read].iter() {
        res += b as usize;
    }

    // should print attempted to kill pid 4097
    _ = kill(read);

    // should print attempted to kill pid 266305
    _ = kill(res);

    shutdown(false);
    loop {}
}
