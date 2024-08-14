use alloc::vec::Vec;
use shared::{
    io::OpenFlags,
    sys::{Sys, SysError},
};

use crate::{
    fs::{path::Path, vfs::Vfs, FsError},
    hart::POWER,
    proc::{Process, ProcessNode, Reg, PROC_LIST},
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

// void kill(u32 pid);
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

// u32 getpid(void);
fn sys_getpid(proc: ProcessNode) -> SysResult {
    Ok(unsafe { proc.with(|p| p.pid as usize) })
}

// uint open(const char *path, uint pathlen, u32 flags);
fn sys_open(proc: ProcessNode, path: VirtAddr, len: usize, flags: u32) -> SysResult {
    // TODO: don't hold the proc lock while doing fs operations

    let Ok(mut buf) = Vec::try_with_capacity(len) else {
        return Err(SysError::NoMem);
    };
    unsafe {
        proc.with(|mut proc| {
            path.copy_from(&proc.pagetable, buf.spare_capacity_mut())?;
            buf.set_len(len);

            let file = Vfs::open_in_cwd(&proc.cwd, &buf[..], OpenFlags::from_bits_truncate(flags))?;
            proc.files
                .push(file)
                .map(|v| v.0)
                .map_err(|_| SysError::NoMem)
        })
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
fn sys_read(proc: ProcessNode, fd: usize, pos: usize, buf: VirtAddr, buflen: usize) -> SysResult {
    unsafe {
        proc.with(|proc| {
            let Some(fd) = proc.files.get(fd).and_then(|f| f.as_ref()) else {
                return Err(SysError::BadFd);
            };

            Ok(fd.read_va(pos as u64, &proc.pagetable, buf, buflen)?)
        })
    }
}

// uint write(uint fd, u64 pos, const char *buf, uint buflen);
fn sys_write(proc: ProcessNode, fd: usize, pos: usize, buf: VirtAddr, buflen: usize) -> SysResult {
    unsafe {
        proc.with(|proc| {
            let Some(fd) = proc.files.get(fd).and_then(|f| f.as_ref()) else {
                return Err(SysError::BadFd);
            };

            Ok(fd.write_va(pos as u64, &proc.pagetable, buf, buflen)?)
        })
    }
}

// bool readdir(uint fd, uint pos, DirEntry *entry);
fn sys_readdir(proc: ProcessNode, fd: usize, pos: usize, entry: VirtAddr) -> SysResult {
    unsafe {
        proc.with(|proc| {
            let Some(fd) = proc.files.get(fd).and_then(|f| f.as_ref()) else {
                return Err(SysError::BadFd);
            };

            let Some(ent) = fd.readdir(pos)? else {
                return Ok(0);
            };

            entry.copy_type_to(&proc.pagetable, &ent, None)?;
            Ok(1)
        })
    }
}

// bool stat(uint fd, Stat *entry);
fn sys_stat(proc: ProcessNode, fd: usize, stat: VirtAddr) -> SysResult {
    unsafe {
        proc.with(|proc| {
            let Some(fd) = proc.files.get(fd).and_then(|f| f.as_ref()) else {
                return Err(SysError::BadFd);
            };
            stat.copy_type_to(&proc.pagetable, &fd.stat()?, None)?;
            Ok(1)
        })
    }
}

// void chdir(const char *path, uint len);
fn sys_chdir(proc: ProcessNode, path: VirtAddr, len: usize) -> SysResult {
    let Ok(mut buf) = Vec::try_with_capacity(len) else {
        return Err(SysError::NoMem);
    };
    unsafe {
        proc.with(|mut proc| {
            path.copy_from(&proc.pagetable, buf.spare_capacity_mut())?;
            buf.set_len(len);

            let cwd = Vfs::open_in_cwd(&proc.cwd, &buf[..], OpenFlags::empty())?;
            if !cwd.vnode().directory {
                return Err(SysError::InvalidArgument);
            }

            proc.cwd = cwd;
            Ok(0)
        })
    }
}

// u32 spawn(const char *path, uint pathlen, const struct string **argv, uint nargs);
fn sys_spawn(
    proc: ProcessNode,
    path: VirtAddr,
    pathlen: usize,
    argv: VirtAddr,
    nargs: usize,
) -> SysResult {
    let Ok(mut buf) = Vec::try_with_capacity(pathlen) else {
        return Err(SysError::NoMem);
    };

    let mut args = Vec::new();
    let Ok(mut arg_slices) = Vec::try_with_capacity(nargs) else {
        return Err(SysError::NoMem);
    };

    let cwd = unsafe {
        proc.with(|proc| {
            path.copy_from(&proc.pagetable, buf.spare_capacity_mut())?;
            buf.set_len(pathlen);

            for i in 0..nargs {
                let ptr: VirtAddr = (argv + (i * 2) * 8).copy_type_from(&proc.pagetable)?;
                let len: usize = (argv + (i * 2 + 1) * 8).copy_type_from(&proc.pagetable)?;
                if args.try_reserve(len).is_err() {
                    return Err(SysError::NoMem);
                }

                ptr.copy_from(&proc.pagetable, &mut args.spare_capacity_mut()[..len])?;
                args.set_len(args.len() + len);
            }

            let mut buf = &args[..];
            for i in 0..nargs {
                let (arg, rest) =
                    buf.split_at((argv + (i * 2 + 1) * 8).copy_type_from(&proc.pagetable)?);
                arg_slices.push(arg);
                buf = rest;
            }

            Ok(proc.cwd.clone())
        })
    }?;

    Process::spawn(Path::new(&buf), cwd, &arg_slices).map(|pid| pid as usize)
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
        Some(Sys::Write) => sys_write(proc, a0, a1, VirtAddr(a2), a3),
        Some(Sys::Readdir) => sys_readdir(proc, a0, a1, VirtAddr(a2)),
        Some(Sys::Chdir) => sys_chdir(proc, VirtAddr(a0), a1),
        Some(Sys::Spawn) => sys_spawn(proc, VirtAddr(a0), a1, VirtAddr(a2), a3),
        Some(Sys::Stat) => sys_stat(proc, a0, VirtAddr(a1)),
        None => Err(SysError::InvalidSyscall),
    };

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
