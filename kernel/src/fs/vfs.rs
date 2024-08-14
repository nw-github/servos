use core::cell::Cell;

use alloc::{
    collections::{btree_map::Entry, BTreeMap},
    sync::Arc,
};
use servos::lock::SpinLocked;

use crate::{
    fs::FsError,
    vmm::{PageTable, VirtAddr},
};

use super::{
    path::{OwnedPath, Path},
    DirEntry, FileSystem, FsResult, OpenFlags, VNode,
};

pub struct FileDescriptor {
    node: VNode,
    dev: Arc<dyn FileSystem>,
    // right now file descriptors can't be shared but if/when they can this might need a spinlock
    pos: Cell<u64>,
}

impl FileDescriptor {
    pub unsafe fn new(node: VNode, dev: Arc<dyn FileSystem>) -> Self {
        Self {
            node,
            dev,
            pos: Cell::new(0),
        }
    }

    pub fn read(&self, pos: u64, buf: &mut [u8]) -> FsResult<usize> {
        if self.node.directory {
            return Err(FsError::InvalidOp);
        }

        self.exec_with_pos(pos, |pos| self.dev.read(&self.node, pos, buf))
    }

    pub fn read_va(&self, pos: u64, pt: &PageTable, va: VirtAddr, len: usize) -> FsResult<usize> {
        if self.node.directory {
            return Err(FsError::InvalidOp);
        }

        self.exec_with_pos(pos, |pos| self.dev.read_va(&self.node, pos, pt, va, len))
    }

    pub fn write_va(&self, pos: u64, pt: &PageTable, va: VirtAddr, len: usize) -> FsResult<usize> {
        if self.node.directory || self.node.readonly {
            return Err(FsError::InvalidOp);
        }

        self.exec_with_pos(pos, |pos| self.dev.write_va(&self.node, pos, pt, va, len))
    }

    pub fn write(&self, pos: u64, buf: &[u8]) -> FsResult<usize> {
        if self.node.directory || self.node.readonly {
            return Err(FsError::ReadOnly);
        }

        self.exec_with_pos(pos, |pos| self.dev.write(&self.node, pos, buf))
    }

    pub fn readdir(&self, cur: usize) -> FsResult<Option<DirEntry>> {
        if cur == usize::MAX {
            let res = self.dev.readdir(&self.node, self.pos.get() as usize);
            self.pos.update(|pos| pos + 1);
            res
        } else {
            self.dev.readdir(&self.node, cur)
        }
    }

    fn exec_with_pos(&self, pos: u64, f: impl FnOnce(u64) -> FsResult<usize>) -> FsResult<usize> {
        if pos == u64::MAX {
            let prev = self.pos.get();
            let res = f(prev)?;
            self.pos.update(|pos| pos + res as u64);
            Ok(res)
        } else {
            f(pos)
        }
    }
}

impl Drop for FileDescriptor {
    fn drop(&mut self) {
        _ = self.dev.close(&self.node);
    }
}

#[derive(Debug)]
pub enum MountError {
    NoMem,
    AlreadyMounted,
}

pub struct Vfs {
    mounts: BTreeMap<OwnedPath, Arc<dyn FileSystem>>,
}

impl Vfs {
    const fn new() -> Self {
        Self {
            mounts: BTreeMap::new(),
        }
    }

    pub fn mount(&mut self, path: OwnedPath, fs: Arc<dyn FileSystem>) -> Result<(), MountError> {
        // TODO: alloc failure
        match self.mounts.entry(path) {
            Entry::Vacant(entry) => {
                entry.insert(fs);
                Ok(())
            }
            Entry::Occupied(_) => Err(MountError::AlreadyMounted),
        }
    }

    pub fn unmount(&mut self, path: &Path) -> bool {
        self.mounts.remove(path).is_some()
    }

    pub fn open(path: impl AsRef<Path>, flags: OpenFlags) -> FsResult<FileDescriptor> {
        fn open(path: &Path, flags: OpenFlags) -> FsResult<FileDescriptor> {
            let Some((dev, path)) = VFS.lock().mounts.iter().rev().find_map(|(mount, dev)| {
                let rest = path.strip_prefix(mount)?;
                Some((dev.clone(), rest))
            }) else {
                return Err(FsError::PathNotFound);
            };

            Ok(unsafe { FileDescriptor::new(dev.open(path, flags)?, dev) })
        }

        open(path.as_ref(), flags)
    }
}

pub static VFS: SpinLocked<Vfs> = SpinLocked::new(Vfs::new());
