use core::alloc::AllocError;

#[derive(strum::FromRepr, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum Sys {
    Shutdown = 1,
    Kill,
    GetPid,
    Open,
    Close,
    Read,
    Write,
    Readdir,
    Chdir,
    Spawn,
    Stat,
    Sbrk,
}

#[derive(strum::FromRepr, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
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
    BadAddr,
    Eof,
}

impl From<AllocError> for SysError {
    fn from(_: AllocError) -> Self {
        Self::NoMem
    }
}

#[cfg(feature = "alloc")]
impl From<alloc::collections::TryReserveError> for SysError {
    fn from(_: alloc::collections::TryReserveError) -> Self {
        Self::NoMem
    }
}
