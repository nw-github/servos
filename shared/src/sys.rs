#[derive(strum::FromRepr, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum Sys {
    Shutdown = 1,
    Kill,
    GetPid,
    Open,
    Close,
    Read,
}

#[derive(strum::FromRepr, Debug, Clone, Copy, PartialEq, Eq)]
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
    BadAddr,
}
