use alloc::vec::Vec;
use shared::sys::{Sys, SysError};

use crate::{
    fs::{vfs::Vfs, FsError, OpenFlags},
    hart::POWER,
    println,
    proc::{ProcessNode, Reg, PROC_LIST},
    vmm::VirtAddr,
};

impl From<FsError> for SysError {
    fn from(value: FsError) -> Self {
        match value {
            FsError::PathNotFound => SysError::PathNotFound,
            FsError::NoMem => SysError::NoMem,
            FsError::ReadOnly => SysError::ReadOnly,
            FsError::InvalidOp => SysError::InvalidOp,
            FsError::Unsupported => SysError::Unsupported,
            FsError::CorruptedFs => SysError::CorruptedFs,
            FsError::InvalidPerms => SysError::InvalidPerms,
            FsError::BadVa => SysError::BadAddr,
        }
    }
}

pub type SysResult = Result<usize, SysError>;

// void shutdown(uint typ);
fn sys_shutdown(_proc: ProcessNode, typ: usize) -> SysResult {
    // TODO: permission check
    match typ {
        0 => POWER.lock().shutdown(),
        1 => POWER.lock().restart(),
        _ => Err(SysError::InvalidArgument),
    }
}

// void kill(uint pid);
fn sys_kill(_proc: ProcessNode, pid: usize) -> SysResult {
    if pid == 0 {
        return Err(SysError::InvalidArgument);
    }

    for proc in PROC_LIST.lock().iter_mut() {
        let success = unsafe {
            proc.with(|mut proc| {
                // TODO: permission check
                if proc.pid as usize == pid {
                    proc.killed = true;
                    true
                } else {
                    false
                }
            })
        };
        if success {
            return Ok(0);
        }
    }

    Err(SysError::NotFound)
}

// uint getpid(void);
fn sys_getpid(proc: ProcessNode) -> SysResult {
    Ok(unsafe { proc.with(|p| p.pid as usize) })
}

// uint open(const char *path, uint pathlen, u32 flags);
fn sys_open(proc: ProcessNode, path: VirtAddr, len: usize, flags: u32) -> SysResult {
    let Ok(mut buf) = Vec::try_with_capacity(len) else {
        return Err(SysError::NoMem);
    };
    unsafe {
        proc.with(|proc| path.copy_from(&proc.pagetable, buf.spare_capacity_mut()))?;
        buf.set_len(len);
    }

    match Vfs::open(&buf[..], OpenFlags::from_bits_truncate(flags)) {
        Ok(file) => {
            let Ok(fd) = (unsafe { proc.with(|mut proc| proc.files.push(file).map(|v| v.0)) })
            else {
                return Err(SysError::NoMem);
            };
            Ok(fd)
        }
        Err(err) => Err(err.into()),
    }
}

// void close(uint fd);
fn sys_close(proc: ProcessNode, fd: usize) -> SysResult {
    unsafe {
        proc.with(|mut proc| {
            if fd >= proc.files.0.len() || proc.files.0[fd].take().is_none() {
                return Err(SysError::BadFd);
            }

            Ok(0)
        })
    }
}

// uint read(uint fd, u64 pos, char *buf, uint buflen);
fn sys_read(proc: ProcessNode, pos: usize, fd: usize, buf: VirtAddr, buflen: usize) -> SysResult {
    unsafe {
        proc.with(|proc| {
            let Some(fd) = proc.files.get(fd).and_then(|f| f.as_ref()) else {
                return Err(SysError::BadFd);
            };

            Ok(fd.read_va(pos as u64, &proc.pagetable, buf, buflen)?)
        })
    }
}

pub fn handle_syscall(proc: ProcessNode) {
    let (syscall_no, a0, a1, a2, a3) = unsafe {
        proc.with(|proc| {
            (
                (*proc.trapframe)[Reg::A7],
                (*proc.trapframe)[Reg::A0],
                (*proc.trapframe)[Reg::A1],
                (*proc.trapframe)[Reg::A2],
                (*proc.trapframe)[Reg::A3],
            )
        })
    };

    let result = match Sys::from_repr(syscall_no) {
        Some(Sys::Shutdown) => sys_shutdown(proc, a0),
        Some(Sys::Kill) => sys_kill(proc, a0),
        Some(Sys::GetPid) => sys_getpid(proc),
        Some(Sys::Open) => sys_open(proc, VirtAddr(a0), a1, (a2 & u32::MAX as usize) as u32),
        Some(Sys::Close) => sys_close(proc, a0),
        Some(Sys::Read) => sys_read(proc, a0, a1, VirtAddr(a2), a3),
        None => Err(SysError::InvalidSyscall),
    };

    if syscall_no == 2 {
        println!("attempting to kill pid {a0}: {}", result.is_ok());
    }

    let (a0, a1) = match result {
        Ok(res) => (res, 0),
        Err(err) => (0, err as usize),
    };
    unsafe {
        proc.with(|mut proc| {
            (*proc.trapframe)[Reg::A0] = a0;
            (*proc.trapframe)[Reg::A1] = a1;
        });
    }
}
