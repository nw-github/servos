use alloc::vec::Vec;
use servos::lock::SpinLocked;
use shared::{
    io::OpenFlags,
    sys::{Sys, SysError},
};

use crate::{
    fs::{path::Path, vfs::Vfs, FsError},
    hart::POWER,
    proc::{Process, Reg, PROC_LIST},
    vmm::{Pte, VirtAddr},
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

type Proc = SpinLocked<Process>;

// void shutdown(uint typ);
fn sys_shutdown(_: &Proc, typ: usize) -> SysResult {
    // TODO: permission check
    match typ {
        0 => POWER.lock().shutdown(),
        1 => POWER.lock().restart(),
        _ => Err(SysError::InvalidArgument),
    }
}

// void kill(u32 pid);
fn sys_kill(_: &Proc, pid: usize) -> SysResult {
    if pid == 0 {
        return Err(SysError::InvalidArgument);
    }

    for &mut proc in PROC_LIST.lock().iter_mut() {
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
fn sys_getpid(proc: &Proc) -> SysResult {
    Ok(proc.lock().pid as usize)
}

// uint open(const char *path, uint pathlen, u32 flags);
fn sys_open(proc: &Proc, path: VirtAddr, len: usize, flags: u32) -> SysResult {
    // TODO: don't hold the proc lock while doing fs operations

    let mut buf = Vec::try_with_capacity(len)?;
    proc.with(|mut proc| {
        path.copy_from(&proc.pagetable, buf.spare_capacity_mut())?;
        unsafe {
            buf.set_len(len);
        }

        let file = Vfs::open_in_cwd(&proc.cwd, &buf[..], OpenFlags::from_bits_truncate(flags))?;
        proc.files
            .push(file)
            .map(|v| v.0)
            .map_err(|_| SysError::NoMem)
    })
}

// void close(uint fd);
fn sys_close(proc: &Proc, fd: usize) -> SysResult {
    proc.with(|mut proc| {
        if fd >= proc.files.0.len() || proc.files.0[fd].take().is_none() {
            return Err(SysError::BadFd);
        }

        Ok(0)
    })
}

// uint read(uint fd, u64 pos, char *buf, uint buflen);
fn sys_read(proc: &Proc, fd: usize, pos: usize, buf: VirtAddr, buflen: usize) -> SysResult {
    proc.with(|proc| {
        let Some(fd) = proc.files.get(fd).and_then(|f| f.as_ref()) else {
            return Err(SysError::BadFd);
        };

        Ok(fd.read_va(pos as u64, &proc.pagetable, buf, buflen)?)
    })
}

// uint write(uint fd, u64 pos, const char *buf, uint buflen);
fn sys_write(proc: &Proc, fd: usize, pos: usize, buf: VirtAddr, buflen: usize) -> SysResult {
    proc.with(|proc| {
        let Some(fd) = proc.files.get(fd).and_then(|f| f.as_ref()) else {
            return Err(SysError::BadFd);
        };

        Ok(fd.write_va(pos as u64, &proc.pagetable, buf, buflen)?)
    })
}

// bool readdir(uint fd, uint pos, DirEntry *entry);
fn sys_readdir(proc: &Proc, fd: usize, pos: usize, entry: VirtAddr) -> SysResult {
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

// bool stat(uint fd, Stat *entry);
fn sys_stat(proc: &Proc, fd: usize, stat: VirtAddr) -> SysResult {
    proc.with(|proc| {
        let Some(fd) = proc.files.get(fd).and_then(|f| f.as_ref()) else {
            return Err(SysError::BadFd);
        };
        stat.copy_type_to(&proc.pagetable, &fd.stat()?, None)?;
        Ok(1)
    })
}

// void chdir(const char *path, uint len);
fn sys_chdir(proc: &Proc, path: VirtAddr, len: usize) -> SysResult {
    let mut buf = Vec::try_with_capacity(len)?;
    proc.with(|mut proc| {
        path.copy_from(&proc.pagetable, buf.spare_capacity_mut())?;
        unsafe {
            buf.set_len(len);
        }

        let cwd = Vfs::open_in_cwd(&proc.cwd, &buf[..], OpenFlags::empty())?;
        if !cwd.vnode().directory {
            return Err(SysError::InvalidArgument);
        }

        proc.cwd = cwd;
        Ok(0)
    })
}

// u32 spawn(const char *path, uint pathlen, const struct string **argv, uint nargs);
fn sys_spawn(
    proc: &Proc,
    path: VirtAddr,
    pathlen: usize,
    argv: VirtAddr,
    nargs: usize,
) -> SysResult {
    let mut buf = Vec::try_with_capacity(pathlen)?;
    let mut args = Vec::new();
    let mut arg_slices = Vec::try_with_capacity(nargs)?;
    let cwd = proc.with(|proc| {
        path.copy_from(&proc.pagetable, buf.spare_capacity_mut())?;
        unsafe {
            buf.set_len(pathlen);
        }

        for i in 0..nargs {
            let ptr: VirtAddr = (argv + (i * 2) * 8).copy_type_from(&proc.pagetable)?;
            let len: usize = (argv + (i * 2 + 1) * 8).copy_type_from(&proc.pagetable)?;
            args.try_reserve(len)?;

            ptr.copy_from(&proc.pagetable, &mut args.spare_capacity_mut()[..len])?;
            unsafe {
                args.set_len(args.len() + len);
            }
        }

        let mut buf = &args[..];
        for i in 0..nargs {
            let (arg, rest) =
                buf.split_at((argv + (i * 2 + 1) * 8).copy_type_from(&proc.pagetable)?);
            arg_slices.push(arg);
            buf = rest;
        }

        Ok::<_, SysError>(proc.cwd.clone())
    })?;

    Process::spawn(Path::new(&buf), cwd, &arg_slices).map(|pid| pid as usize)
}

// void *sbrk(isize inc);
fn sys_sbrk(proc: &Proc, inc: isize) -> SysResult {
    let mut proc = proc.lock();
    let cur_brk = proc.brk;
    let Some(mut new_brk) = cur_brk.0.checked_add_signed(inc).map(VirtAddr) else {
        return Err(SysError::InvalidArgument);
    };

    if new_brk.page() != cur_brk {
        new_brk = new_brk.next_page();
        if inc < 0 {
            proc.pagetable.unmap_pages(new_brk, cur_brk.0 - new_brk.0);
        } else if !proc.pagetable.map_new_pages(cur_brk, new_brk.0 - cur_brk.0, Pte::Urw) {
            return Err(SysError::NoMem);
        }
        proc.brk = new_brk;
    }
    Ok(proc.brk.0)
}

pub fn handle_syscall(proc: &Proc) {
    let (syscall_no, a0, a1, a2, a3) = proc.with(|mut proc| {
        let trapframe = proc.trapframe();
        (
            trapframe[Reg::A7],
            trapframe[Reg::A0],
            trapframe[Reg::A1],
            trapframe[Reg::A2],
            trapframe[Reg::A3],
        )
    });

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
        Some(Sys::Sbrk) => sys_sbrk(proc, a0 as isize),
        None => Err(SysError::InvalidSyscall),
    };

    let (a0, a1) = match result {
        Ok(res) => (res, 0),
        Err(err) => (0, err as usize),
    };
    let mut proc = proc.lock();
    proc.trapframe()[Reg::A0] = a0;
    proc.trapframe()[Reg::A1] = a1;
}
