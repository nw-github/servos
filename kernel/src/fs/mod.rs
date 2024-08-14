use core::mem::MaybeUninit;

use path::Path;
use shared::io::{DirEntry, OpenFlags, Stat};

use crate::vmm::{PageTable, Pte, VirtAddr, VirtToPhysErr};

pub mod dev;
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

impl From<VirtToPhysErr> for FsError {
    fn from(_: VirtToPhysErr) -> Self {
        Self::BadVa
    }
}

#[derive(Clone)]
pub struct VNode {
    pub ino: u64,
    pub directory: bool,
    pub readonly: bool,
}

pub trait FileSystem {
    fn open(&self, path: &Path, flags: OpenFlags, cwd: Option<&VNode>) -> FsResult<VNode>;
    fn read<'a>(
        &self,
        vn: &VNode,
        pos: u64,
        buf: &'a mut [MaybeUninit<u8>],
    ) -> FsResult<&'a mut [u8]>;
    fn write(&self, vn: &VNode, pos: u64, buf: &[u8]) -> FsResult<usize>;
    fn close(&self, vn: &VNode) -> FsResult<()>;
    fn readdir(&self, vn: &VNode, pos: usize) -> FsResult<Option<DirEntry>>;
    fn stat(&self, vn: &VNode) -> FsResult<Stat>;

    fn read_va(
        &self,
        vn: &VNode,
        pos: u64,
        pt: &PageTable,
        buf: VirtAddr,
        len: usize,
    ) -> FsResult<usize> {
        rw_va(pos, pt, buf, len, Pte::U | Pte::W, |pos, buf| {
            let buf_uninit =
                unsafe { core::slice::from_raw_parts_mut(buf.as_mut_ptr().cast(), buf.len()) };
            self.read(vn, pos, buf_uninit).map(|v| v.len())
        })
    }

    fn write_va(
        &self,
        vn: &VNode,
        pos: u64,
        pt: &PageTable,
        buf: VirtAddr,
        len: usize,
    ) -> FsResult<usize> {
        rw_va(pos, pt, buf, len, Pte::U | Pte::R, |pos, buf| {
            self.write(vn, pos, buf)
        })
    }

    // fn rename(&self, vn: &VNode, abspath: &Path, mvdir: bool) -> FsResult<()>;
}

fn rw_va(
    mut pos: u64,
    pt: &PageTable,
    buf: VirtAddr,
    len: usize,
    perms: Pte,
    mut f: impl FnMut(u64, &mut [u8]) -> FsResult<usize>,
) -> FsResult<usize> {
    let mut total = 0;
    for phys in buf.iter_phys(pt, len, perms) {
        let phys = phys.map(|r| unsafe { core::slice::from_mut_ptr_range(r) })?;
        let read = f(pos, phys)?;
        total += read;
        if read < phys.len() {
            break;
        }
        pos += read as u64;
    }

    Ok(total)
}
