use crate::{
    hart::POWER,
    println,
    proc::{ProcessNode, Reg, PROC_LIST},
};

#[derive(strum::FromRepr, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum Sys {
    Shutdown = 1,
    Kill,
    GetPid,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(isize)]
pub enum SysError {
    InvalidSyscall = 1,
    InvalidArgument,
    NotFound,
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

pub fn sys_shutdown(_proc: ProcessNode, typ: usize) -> SysResult {
    // TODO: permission check
    match typ {
        0 => POWER.lock().shutdown(),
        1 => POWER.lock().restart(),
        _ => SysResult::err(SysError::InvalidArgument),
    }
}

pub fn sys_kill(_proc: ProcessNode, pid: usize) -> SysResult {
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

pub fn sys_getpid(proc: ProcessNode) -> SysResult {
    SysResult::ok(unsafe { proc.with(|p| p.pid as isize) })
}

pub fn handle_syscall(proc: ProcessNode) {
    let (syscall_no, a0) =
        unsafe { proc.with(|proc| ((*proc.trapframe)[Reg::A7], (*proc.trapframe)[Reg::A0])) };

    let result = match Sys::from_repr(syscall_no) {
        Some(Sys::Shutdown) => sys_shutdown(proc, a0),
        Some(Sys::Kill) => sys_kill(proc, a0),
        Some(Sys::GetPid) => sys_getpid(proc),
        _ => SysResult::err(SysError::InvalidSyscall),
    };

    if syscall_no == 2 {
        println!("attempting to kill pid {a0}: {}", result.is_ok());
    }

    unsafe {
        proc.with(|mut proc| (*proc.trapframe)[Reg::A0] = result.0);
    }
}
