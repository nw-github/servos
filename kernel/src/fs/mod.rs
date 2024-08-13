use bitflags::bitflags;
use path::Path;

use crate::vmm::{page_offset, PageTable, Pte, VirtAddr, PGSIZE};

pub mod initrd;
pub mod path;
pub mod vfs;

pub type FsResult<T> = Result<T, FsError>;

#[derive(Debug)]
pub enum FsError {
    PathNotFound,
    NoMem,
    ReadOnly,
    InvalidOp,
    Unsupported,
    CorruptedFs,
    InvalidPerms,
    BadVa,
}

pub struct VNode {
    ino: u64,
    directory: bool,
    readonly: bool,
}

pub struct DirEntry {
    name: [u8; 0x100],
    ino: u64,
    directory: bool,
    readonly: bool,
}

pub trait FileSystem {
    /// Open a path. `path` should be the components after the mount point
    fn open(&self, path: &Path, flags: OpenFlags) -> FsResult<VNode>;
    fn read(&self, vn: &VNode, pos: u64, buf: &mut [u8]) -> FsResult<usize>;
    fn write(&self, vn: &VNode, pos: u64, buf: &[u8]) -> FsResult<usize>;
    fn close(&self, vn: &VNode) -> FsResult<()>;
    fn get_dir_entry(&self, vn: &VNode, pos: usize) -> FsResult<Option<DirEntry>>;

    fn read_va(
        &self,
        vn: &VNode,
        mut pos: u64,
        pt: &PageTable,
        buf: VirtAddr,
        mut len: usize,
    ) -> FsResult<usize> {
        let mut total = 0;
        while len != 0 {
            let Some(pa) = buf.to_phys(pt, Pte::U | Pte::W) else {
                return Err(FsError::BadVa);
            };

            let buf = unsafe {
                core::slice::from_raw_parts_mut(
                    pa.0 as *mut u8,
                    (PGSIZE - page_offset(pa.0)).min(len),
                )
            };
            let read = self.read(vn, pos, buf)?;
            total += read;
            if read < buf.len() {
                break;
            }

            len -= read;
            pos += read as u64;
        }

        Ok(total)
    }

    // fn rename(&self, vn: &VNode, abspath: &Path, mvdir: bool) -> FsResult<()>;
}

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
