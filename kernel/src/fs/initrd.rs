use core::mem::{size_of, MaybeUninit};

use alloc::{boxed::Box, vec::Vec};
use shared::io::{DirEntry, OpenFlags, Stat};

use super::{path::Path, FileSystem, FsError, FsResult, VNode};

pub const INITRD_MAGIC: u32 = 0xce3fdefe;

#[allow(unused)]
pub const INODE_FILE: u16 = 0;
pub const INODE_DIR: u16 = 1;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InitRdHeader {
    magic: u32,
    _reserved: u32,
    ninodes: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct INode {
    name: [u8; 32],
    nlen: u16,
    typ: u16,
    /// For a file, this is the size of the file. For a directory, this is the number of entries.
    size: u32,
    addr: u64,
}

impl INode {
    pub fn name_eq(&self, name: &[u8]) -> bool {
        if self.nlen as usize > self.name.len() {
            return false;
        }

        &self.name[..self.nlen as usize] == name
    }
}

pub struct InitRd {
    inodes: Box<[INode]>,
    data: Box<[u8]>,
}

impl InitRd {
    pub fn new(data: &[u8]) -> Option<Self> {
        assert!(data.as_ptr().is_aligned_to(align_of::<InitRdHeader>()));

        let (header, data) = data.split_at_checked(size_of::<InitRdHeader>())?;
        let header = unsafe { &*header.as_ptr().cast::<InitRdHeader>() };
        if header.magic != INITRD_MAGIC {
            return None;
        }

        let (inodes, files) =
            data.split_at_checked(header.ninodes as usize * size_of::<INode>())?;
        let inodes = try_vec_from_slice(unsafe {
            core::slice::from_raw_parts(inodes.as_ptr().cast::<INode>(), header.ninodes as usize)
        })?;
        if !inodes.first().is_some_and(|i| i.typ == INODE_DIR) {
            return None;
        }

        Some(Self {
            inodes: inodes.into(),
            data: try_vec_from_slice(files)?.into(),
        })
    }

    fn dir_entry(&self, dir: &INode, pos: usize) -> Option<(usize, &INode)> {
        const U64SZ: usize = size_of::<u64>();
        if pos >= dir.size as usize {
            return None;
        }

        let addr = dir.addr as usize + pos * U64SZ;
        let entry = u64::from_le_bytes(self.data.get(addr..addr + U64SZ)?.try_into().unwrap());
        let entry: usize = entry.try_into().ok()?;
        Some((entry, self.inodes.get(entry)?))
    }

    fn vnode_to_inode(&self, vn: &VNode) -> FsResult<&INode> {
        self.inodes.get(vn.ino as usize).ok_or(FsError::CorruptedFs)
    }

    fn stat_inode(inode: &INode) -> Stat {
        Stat {
            size: inode.size as usize,
            readonly: true,
            directory: inode.typ == INODE_DIR,
        }
    }
}

impl FileSystem for InitRd {
    fn open(&self, path: &Path, _flags: OpenFlags, root: Option<&VNode>) -> FsResult<VNode> {
        let mut ino = root
            .filter(|_| !path.is_absolute())
            .map(|r| r.ino as usize)
            .unwrap_or(0);
        'outer: for component in path.components() {
            if self.inodes[ino].typ != INODE_DIR {
                return Err(FsError::PathNotFound);
            }

            for i in 0..self.inodes[ino].size {
                let (entry_no, inode) = self
                    .dir_entry(&self.inodes[ino], i as usize)
                    .ok_or(FsError::CorruptedFs)?;
                if inode.name_eq(component) {
                    ino = entry_no;
                    continue 'outer;
                }
            }

            return Err(FsError::PathNotFound);
        }

        Ok(VNode {
            ino: ino as u64,
            directory: self.inodes[ino].typ == INODE_DIR,
            readonly: true,
        })
    }

    fn read<'a>(
        &self,
        vn: &VNode,
        pos: u64,
        buf: &'a mut [MaybeUninit<u8>],
    ) -> FsResult<&'a mut [u8]> {
        let inode = self.vnode_to_inode(vn)?;
        if inode.typ == INODE_DIR {
            return Err(FsError::InvalidOp);
        }

        let Some(len) = (inode.size as u64).checked_sub(pos).filter(|&len| len != 0) else {
            return Err(FsError::Eof);
        };

        let len = buf.len().min(len as usize);
        unsafe {
            core::ptr::copy_nonoverlapping(
                self.data[inode.addr as usize + pos as usize..][..len].as_ptr(),
                buf.as_mut_ptr().cast(),
                len,
            );
        }
        Ok(unsafe { MaybeUninit::slice_assume_init_mut(&mut buf[..len]) })
    }

    fn write(&self, _vn: &VNode, _pos: u64, _buf: &[u8]) -> FsResult<usize> {
        Err(FsError::Unsupported)
    }

    fn close(&self, _vn: &VNode) -> FsResult<()> {
        Ok(())
    }

    fn readdir(&self, vn: &VNode, pos: usize) -> FsResult<Option<DirEntry>> {
        let dir = self.vnode_to_inode(vn)?;
        if dir.typ != INODE_DIR {
            return Err(FsError::InvalidOp);
        }

        let Some((_, inode)) = self.dir_entry(dir, pos) else {
            return Ok(None);
        };

        let mut entry = DirEntry {
            name: [0; 0x100],
            name_len: inode.nlen as usize,
            stat: Self::stat_inode(inode),
        };
        entry.name[..inode.name.len()].copy_from_slice(&inode.name);

        Ok(Some(entry))
    }

    fn stat(&self, vn: &VNode) -> FsResult<Stat> {
        Ok(Self::stat_inode(
            self.inodes
                .get(vn.ino as usize)
                .ok_or(FsError::CorruptedFs)?,
        ))
    }
}

fn try_vec_from_slice<T: Clone>(slc: &[T]) -> Option<Vec<T>> {
    let mut vec = Vec::try_with_capacity(slc.len()).ok()?;
    vec.extend(slc.iter().cloned());
    Some(vec)
}
