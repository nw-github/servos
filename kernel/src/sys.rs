use alloc::vec::Vec;
use servos::lock::SpinLocked;
use shared::{
    io::{DirEntry, OpenFlags, Stat},
    sys::{Sys, SysError as E},
};

use crate::{
    fs::{path::Path, vfs::Vfs, FsError},
    power::POWER,
    proc::{ProcStatus, Process, Reg, PROC_LIST},
    vmm::{Pte, User, VirtAddr},
};

impl From<FsError> for E {
    fn from(value: FsError) -> Self {
        match value {
            FsError::PathNotFound => E::PathNotFound,
            FsError::NoMem => E::NoMem,
            FsError::ReadOnly => E::ReadOnly,
            FsError::InvalidOp => E::InvalidOp,
            FsError::Unsupported => E::Unsupported,
            FsError::CorruptedFs => E::CorruptedFs,
            FsError::InvalidPerms => E::InvalidPerms,
            FsError::BadVa => E::BadAddr,
            FsError::Eof => E::Eof,
        }
    }
}

pub type SysResult = Result<usize, E>;

type Proc = SpinLocked<Process>;

// void shutdown(uint typ);
fn sys_shutdown(_: &Proc, typ: usize) -> SysResult {
    // TODO: permission check
    match typ {
        0 => POWER.lock().shutdown(),
        1 => POWER.lock().restart(),
        _ => Err(E::BadArg),
    }
}

// void kill(u32 pid);
fn sys_kill(_: &Proc, pid: usize) -> SysResult {
    if pid == 0 {
        return Err(E::BadArg);
    }

    for proc in PROC_LIST.lock().iter() {
        let success = unsafe {
            proc.with(|mut proc| {
                // TODO: permission check
                if proc.pid as usize == pid {
                    proc.kill(None);
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

    Err(E::NotFound)
}

// u32 getpid(void);
fn sys_getpid(proc: &Proc) -> SysResult {
    Ok(proc.lock().pid as usize)
}

// uint open(const u8 *path, uint pathlen, u32 flags);
fn sys_open(proc: &Proc, path: VirtAddr, len: usize, flags: u32) -> SysResult {
    let mut buf = Vec::try_with_capacity(len)?;
    proc.with(|mut proc| {
        path.copy_from(proc.pagetable(), buf.spare_capacity_mut())?;
        unsafe {
            buf.set_len(len);
        }

        let file = Vfs::open_in_cwd(&proc.cwd, &buf[..], OpenFlags::from_bits_truncate(flags))?;
        proc.files.push(file).map(|v| v.0).map_err(|_| E::NoMem)
    })
}

// void close(uint fd);
fn sys_close(proc: &Proc, fd: usize) -> SysResult {
    proc.with(|mut proc| proc.files.remove(fd).ok_or(E::BadFd).map(|_| 0))
}

// uint read(uint fd, u64 pos, u8 *buf, uint buflen);
fn sys_read(proc: &Proc, fd: usize, pos: usize, buf: VirtAddr, buflen: usize) -> SysResult {
    proc.with(|proc| {
        Ok(proc.files.get(fd).ok_or(E::BadFd)?.read_va(
            pos as u64,
            proc.pagetable(),
            buf,
            buflen,
        )?)
    })
}

// uint write(uint fd, u64 pos, const u8 *buf, uint buflen);
fn sys_write(proc: &Proc, fd: usize, pos: usize, buf: VirtAddr, buflen: usize) -> SysResult {
    proc.with(|proc| {
        Ok(proc.files.get(fd).ok_or(E::BadFd)?.write_va(
            pos as u64,
            proc.pagetable(),
            buf,
            buflen,
        )?)
    })
}

// bool readdir(uint fd, uint pos, struct DirEntry *entry);
fn sys_readdir(proc: &Proc, fd: usize, pos: usize, entry: User<DirEntry>) -> SysResult {
    proc.with(|proc| {
        let Some(ent) = proc.files.get(fd).ok_or(E::BadFd)?.readdir(pos)? else {
            return Ok(0);
        };

        entry.write(proc.pagetable(), &ent)?;
        Ok(1)
    })
}

// void stat(uint fd, struct Stat *entry);
fn sys_stat(proc: &Proc, fd: usize, stat: User<Stat>) -> SysResult {
    proc.with(|proc| {
        stat.write(
            proc.pagetable(),
            &proc.files.get(fd).ok_or(E::BadFd)?.stat()?,
        )?;
        Ok(0)
    })
}

// void chdir(const u8 *path, uint len);
fn sys_chdir(proc: &Proc, path: VirtAddr, len: usize) -> SysResult {
    let mut buf = Vec::try_with_capacity(len)?;
    proc.with(|mut proc| {
        path.copy_from(proc.pagetable(), buf.spare_capacity_mut())?;
        unsafe {
            buf.set_len(len);
        }

        let cwd = Vfs::open_in_cwd(&proc.cwd, &buf[..], OpenFlags::empty())?;
        if !cwd.vnode().directory {
            return Err(E::BadArg);
        }

        proc.cwd = cwd;
        Ok(0)
    })
}

#[repr(C)]
#[derive(Clone, Copy)]
struct KString {
    ptr: VirtAddr,
    len: usize,
}

// u32 spawn(const u8 *path, uint pathlen, const struct KString **argv, uint nargs);
fn sys_spawn(
    proc: &Proc,
    path: VirtAddr,
    pathlen: usize,
    argv: User<KString>,
    nargs: usize,
) -> SysResult {
    let mut buf = Vec::try_with_capacity(pathlen)?;
    let mut args = Vec::new();
    let mut arg_slices = Vec::try_with_capacity(nargs)?;
    let cwd = proc.with(|proc| {
        path.copy_from(proc.pagetable(), buf.spare_capacity_mut())?;
        unsafe {
            buf.set_len(pathlen);
        }

        for i in 0..nargs {
            let str = argv.read_nth(proc.pagetable(), i)?;
            args.try_reserve(str.len)?;

            str.ptr
                .copy_from(proc.pagetable(), &mut args.spare_capacity_mut()[..str.len])?;
            unsafe {
                args.set_len(args.len() + str.len);
            }
        }

        let mut buf = &args[..];
        for i in 0..nargs {
            let (arg, rest) = buf.split_at(argv.read_nth(proc.pagetable(), i).unwrap().len);
            arg_slices.push(arg);
            buf = rest;
        }

        Ok::<_, E>(proc.cwd.clone())
    })?;

    Process::spawn(Path::new(&buf), cwd, &arg_slices).map(|pid| pid as usize)
}

// usize waitpid(u32 pid);
fn sys_waitpid(proc: &Proc, pid: usize) -> SysResult {
    if proc.lock().pid as usize == pid {
        return Err(E::BadArg);
    }

    for &rhs in PROC_LIST.lock().iter() {
        if unsafe { rhs.with(|proc| proc.pid as usize == pid) } {
            proc.lock().status = ProcStatus::Waiting(pid as u32);
            break;
        }
    }

    Ok(0)
}

// void *sbrk(sint inc);
fn sys_sbrk(proc: &Proc, inc: isize) -> SysResult {
    let mut proc = proc.lock();
    let cur_brk = proc.brk;
    let Some(new_brk) = cur_brk.0.checked_add_signed(inc).map(VirtAddr) else {
        return Err(E::BadArg);
    };

    if !(new_brk.page() == cur_brk.page() || (inc == 1 && new_brk.page() != cur_brk.page())) {
        let pt = proc.pagetable_mut();
        if inc < 0 {
            pt.unmap_pages(new_brk.next_page(), cur_brk);
        } else if !pt.map_new_pages(
            cur_brk.next_page(),
            new_brk.0 - cur_brk.next_page().0,
            Pte::Urw,
            true,
        ) {
            return Err(E::NoMem);
        }
    }

    proc.brk = new_brk;
    Ok(proc.brk.0)
}

// void exit(usize ec);
fn sys_exit(proc: &Proc, ecode: usize) -> SysResult {
    proc.lock().kill(Some(ecode));
    Ok(0)
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
        Some(Sys::Readdir) => sys_readdir(proc, a0, a1, VirtAddr(a2).into()),
        Some(Sys::Chdir) => sys_chdir(proc, VirtAddr(a0), a1),
        Some(Sys::Spawn) => sys_spawn(proc, VirtAddr(a0), a1, VirtAddr(a2).into(), a3),
        Some(Sys::Stat) => sys_stat(proc, a0, VirtAddr(a1).into()),
        Some(Sys::Sbrk) => sys_sbrk(proc, a0 as isize),
        Some(Sys::Waitpid) => sys_waitpid(proc, a0),
        Some(Sys::Exit) => sys_exit(proc, a0),
        None => Err(E::BadSyscall),
    };

    let (a0, a1) = match result {
        Ok(res) => (res, 0),
        Err(err) => (0, err as usize),
    };
    let mut proc = proc.lock();
    proc.trapframe()[Reg::A0] = a0;
    proc.trapframe()[Reg::A1] = a1;
}
