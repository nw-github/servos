use alloc::{
    boxed::Box,
    collections::{btree_map::Entry, BTreeMap},
    sync::Arc,
};
use servos::lock::SpinLocked;

use crate::fs::FsError;

use super::{FileSystem, FsResult, OpenFlags, VNode};

pub struct FileRef {
    node: VNode,
    dev: Arc<dyn FileSystem>,
}

impl FileRef {
    pub unsafe fn new(node: VNode, dev: Arc<dyn FileSystem>) -> Self {
        Self { node, dev }
    }

    pub fn read(&self, pos: u64, buf: &mut [u8]) -> FsResult<usize> {
        if self.node.directory {
            return Err(FsError::InvalidOp);
        }

        self.dev.read(&self.node, pos, buf)
    }

    pub fn write(&self, pos: u64, buf: &[u8]) -> FsResult<usize> {
        if self.node.readonly {
            return Err(FsError::ReadOnly);
        }

        self.dev.write(&self.node, pos, buf)
    }
}

impl Drop for FileRef {
    fn drop(&mut self) {
        _ = self.dev.close(&self.node);
    }
}

pub enum MountError<T> {
    NoMem(T),
    AlreadyMounted(T),
}

pub struct Vfs {
    mounts: BTreeMap<Box<[u8]>, Arc<dyn FileSystem>>,
}

impl Vfs {
    const fn new() -> Self {
        Self {
            mounts: BTreeMap::new(),
        }
    }

    pub fn mount<T: FileSystem + 'static>(
        &mut self,
        path: Box<[u8]>,
        fs: T,
    ) -> Result<(), MountError<T>> {
        // TODO: alloc failure
        match self.mounts.entry(path) {
            Entry::Vacant(entry) => {
                entry.insert(Arc::new(fs));
                Ok(())
            }
            Entry::Occupied(_) => Err(MountError::AlreadyMounted(fs)),
        }
    }

    pub fn unmount(&mut self, path: &[u8]) -> bool {
        self.mounts.remove(path).is_some()
    }

    pub fn open(path: &[u8], flags: OpenFlags) -> FsResult<FileRef> {
        let Some((dev, path)) = VFS.lock().mounts.iter().find_map(|(mount, dev)| {
            let (prefix, post) = path.split_at_checked(mount.len())?;
            (prefix == &**mount).then(|| (dev.clone(), post))
        }) else {
            return Err(FsError::PathNotFound);
        };

        Ok(unsafe { FileRef::new(dev.open(path, flags)?, dev) })
    }
}

pub static VFS: SpinLocked<Vfs> = SpinLocked::new(Vfs::new());
