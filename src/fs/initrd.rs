use core::mem::size_of;

use alloc::{boxed::Box, vec::Vec};

use super::{FileSystem, FsError, FsResult, VNode};

pub const INITRD_MAGIC: u32 = 0xce3fdefe;

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
    pub fn new(mut data: &[u8]) -> Option<Self> {
        assert!(data.as_ptr().is_aligned_to(0x8));

        let header = read_struct::<InitRdHeader>(&mut data)?;
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
}

impl FileSystem for InitRd {
    fn open(&self, path: &[u8], _flags: super::OpenFlags) -> FsResult<VNode> {
        let mut ino = 0;
        for component in path.split(|&c| c == b'/') {
            if self.inodes[ino].typ != INODE_DIR {
                return Err(FsError::PathNotFound);
            }

            for i in 0..self.inodes[ino].size {
                const U64SZ: usize = size_of::<u64>();

                let addr = self.inodes[ino].addr as usize + i as usize * U64SZ;
                let entry = u64::from_le_bytes(
                    self.data
                        .get(addr..addr + U64SZ)
                        .ok_or(FsError::CorruptedFs)?
                        .try_into()
                        .unwrap(),
                );
                let entry: usize = entry.try_into().map_err(|_| FsError::CorruptedFs)?;
                if self
                    .inodes
                    .get(entry)
                    .ok_or(FsError::CorruptedFs)?
                    .name_eq(component)
                {
                    ino = entry;
                    break;
                }
            }
        }

        Ok(VNode {
            ino: ino as u64,
            directory: false,
            readonly: true,
        })
    }

    fn read(&self, vn: &VNode, pos: u64, buf: &mut [u8]) -> FsResult<usize> {
        let inode = self
            .inodes
            .get(vn.ino as usize)
            .ok_or(FsError::CorruptedFs)?;
        if inode.typ == INODE_DIR {
            return Err(FsError::InvalidOp);
        }

        let Some(len) = (inode.size as u64).checked_sub(pos) else {
            return Ok(0);
        };

        let len = buf.len().min(len as usize);
        // maybe do an unchecked copy for speed
        buf[..len].copy_from_slice(&self.data[inode.addr as usize + pos as usize..][..len]);
        Ok(len)
    }

    fn write(&self, _vn: &VNode, _pos: u64, _buf: &[u8]) -> FsResult<usize> {
        Err(FsError::Unsupported)
    }

    fn close(&self, _vn: &VNode) -> FsResult<()> {
        Ok(())
    }
}

fn read_struct<T: Copy>(data: &mut &[u8]) -> Option<T> {
    let (head, rest) = data.split_at_checked(size_of::<T>())?;
    *data = rest;
    unsafe { Some(*head.as_ptr().cast::<T>()) }
}

fn try_vec_from_slice<T: Clone>(slc: &[T]) -> Option<Vec<T>> {
    let mut vec = Vec::try_with_capacity(slc.len()).ok()?;
    vec.extend(slc.iter().cloned());
    Some(vec)
}
