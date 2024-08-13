use alloc::vec::Vec;

use crate::{
    fs::{vfs::Vfs, FsError, OpenFlags},
    hart::POWER,
    println,
    proc::{ProcessNode, Reg, PROC_LIST},
    vmm::VirtAddr,
};

#[derive(strum::FromRepr, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum Sys {
    Shutdown = 1,
    Kill,
    GetPid,
    Open,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(isize)]
pub enum SysError {
    InvalidSyscall = 1,
    InvalidArgument,
    NotFound,
    BadFd,
    NoMem,
    PathNotFound,
    ReadOnly,
    InvalidOp,
    Unsupported,
    CorruptedFs,
    InvalidPerms,
}

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
        }
    }
}

pub struct SysResult(usize);

impl SysResult {
    pub const fn err(err: SysError) -> Self {
        Self(-(err as isize) as usize)
    }

    pub const fn ok(res: isize) -> Self {
        assert!(res & (1 << (usize::BITS - 1)) == 0);
        Self(res as usize)
    }

    pub fn is_ok(&self) -> bool {
        self.0 & (1 << (usize::BITS - 1)) == 0
    }

    pub fn is_err(&self) -> bool {
        self.0 & (1 << (usize::BITS - 1)) != 0
    }
}

// sint shutdown(uint typ);
fn sys_shutdown(_proc: ProcessNode, typ: usize) -> SysResult {
    // TODO: permission check
    match typ {
        0 => POWER.lock().shutdown(),
        1 => POWER.lock().restart(),
        _ => SysResult::err(SysError::InvalidArgument),
    }
}

// sint kill(uint pid);
fn sys_kill(_proc: ProcessNode, pid: usize) -> SysResult {
    if pid == 0 {
        return SysResult::err(SysError::InvalidArgument);
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
            return SysResult::ok(0);
        }
    }

    SysResult::err(SysError::NotFound)
}

// sint getpid(void);
fn sys_getpid(proc: ProcessNode) -> SysResult {
    SysResult::ok(unsafe { proc.with(|p| p.pid as isize) })
}

// sint open(const char *path, uint pathlen, u32 flags);
fn sys_open(proc: ProcessNode, path: VirtAddr, len: usize, flags: u32) -> SysResult {
    let Ok(mut buf) = Vec::try_with_capacity(len) else {
        return SysResult::err(SysError::NoMem);
    };
    unsafe {
        if !proc.with(|proc| path.ucopy_from(&proc.pagetable, buf.spare_capacity_mut())) {
            return SysResult::err(SysError::InvalidArgument);
        }

        buf.set_len(len);
    }

    match Vfs::open(&buf[..], OpenFlags::from_bits_truncate(flags)) {
        Ok(file) => {
            let Ok(fd) = (unsafe { proc.with(|mut proc| proc.files.push(file).map(|v| v.0)) })
            else {
                return SysResult::err(SysError::NoMem);
            };
            SysResult::ok(fd as isize)
        }
        Err(err) => SysResult::err(err.into()),
    }
}

pub fn handle_syscall(proc: ProcessNode) {
    let (syscall_no, a0, a1, a2) = unsafe {
        proc.with(|proc| {
            (
                (*proc.trapframe)[Reg::A7],
                (*proc.trapframe)[Reg::A0],
                (*proc.trapframe)[Reg::A1],
                (*proc.trapframe)[Reg::A2],
            )
        })
    };

    let result = match Sys::from_repr(syscall_no) {
        Some(Sys::Shutdown) => sys_shutdown(proc, a0),
        Some(Sys::Kill) => sys_kill(proc, a0),
        Some(Sys::GetPid) => sys_getpid(proc),
        Some(Sys::Open) => sys_open(proc, VirtAddr(a0), a1, (a2 & u32::MAX as usize) as u32),
        None => SysResult::err(SysError::InvalidSyscall),
    };

    if syscall_no == 2 {
        println!("attempting to kill pid {a0}: {}", result.is_ok());
    }

    unsafe {
        proc.with(|mut proc| (*proc.trapframe)[Reg::A0] = result.0);
    }
}
