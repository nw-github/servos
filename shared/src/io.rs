use bitflags::bitflags;

bitflags! {
    pub struct OpenFlags: u32 {
        /// Create a directory if it doesn't exist
        const CreateDir = 1 << 0;
        /// Create the file if it doesn't exist
        const CreateFile = 1 << 1;
        /// Allow reading and writing
        const ReadWrite = 1 << 2;
        /// Truncate the file to zero when opening
        const Truncate = 1 << 3;
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DirEntry {
    pub name: [u8; 0x100],
    pub name_len: usize,
    pub size: usize,
    pub readonly: bool,
    pub directory: bool,
}
