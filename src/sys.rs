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
pub enum SyscallError {
    InvalidSyscall = 1,
    InvalidArgument,
    ResourceNotFound,
}

pub fn sys_shutdown(_proc: ProcessNode, typ: usize) -> Result<isize, SyscallError> {
    // TODO: permission check
    match typ {
        0 => POWER.lock().shutdown(),
        1 => POWER.lock().restart(),
        _ => Err(SyscallError::InvalidArgument),
    }
}

pub fn sys_kill(_proc: ProcessNode, pid: usize) -> Result<isize, SyscallError> {
    if pid == 0 {
        return Err(SyscallError::InvalidArgument);
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

    Err(SyscallError::ResourceNotFound)
}

pub fn sys_getpid(proc: ProcessNode) -> Result<isize, SyscallError> {
    Ok(unsafe { proc.with(|p| p.pid as isize) })
}

pub fn handle_syscall(proc: ProcessNode) {
    let (syscall_no, a0) =
        unsafe { proc.with(|proc| ((*proc.trapframe)[Reg::A7], (*proc.trapframe)[Reg::A0])) };
    // println!(
    //     "Syscall number {syscall_no} from PID {} on hart {}",
    //     unsafe { proc.with(|p| p.pid) },
    //     r_tp(),
    // );

    let result = match Sys::from_repr(syscall_no) {
        Some(Sys::Shutdown) => sys_shutdown(proc, a0),
        Some(Sys::Kill) => sys_kill(proc, a0),
        Some(Sys::GetPid) => sys_getpid(proc),
        _ => Err(SyscallError::InvalidSyscall),
    };

    if syscall_no == 2 {
        println!("attempting to kill pid {a0}: {}", result.is_ok());
    }

    unsafe {
        proc.with(|mut proc| {
            (*proc.trapframe)[Reg::A0] = match result {
                Ok(res) => {
                    assert!(res & (1 << (usize::BITS - 1)) == 0);
                    res as usize
                }
                Err(err) => -(err as isize) as usize,
            };
        })
    }
}
