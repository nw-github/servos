use alloc::{
    collections::{btree_map::Entry, BTreeMap},
    sync::Arc,
};
use servos::lock::SpinLocked;

use crate::{fs::FsError, vmm::{PageTable, VirtAddr}};

use super::{
    path::{OwnedPath, Path}, DirEntry, FileSystem, FsResult, OpenFlags, VNode
};

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

    pub fn read_va(&self, pos: u64, pt: &PageTable, va: VirtAddr, len: usize) -> FsResult<usize> {
        if self.node.directory {
            return Err(FsError::InvalidOp);
        }

        self.dev.read_va(&self.node, pos, pt, va, len)
    }

    pub fn write(&self, pos: u64, buf: &[u8]) -> FsResult<usize> {
        if self.node.readonly {
            return Err(FsError::ReadOnly);
        }

        self.dev.write(&self.node, pos, buf)
    }

    pub fn get_dir_entry(&self, cur: usize) -> FsResult<Option<DirEntry>> {
        self.dev.get_dir_entry(&self.node, cur)
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
    mounts: BTreeMap<OwnedPath, Arc<dyn FileSystem>>,
}

impl Vfs {
    const fn new() -> Self {
        Self {
            mounts: BTreeMap::new(),
        }
    }

    pub fn mount<T: FileSystem + 'static>(
        &mut self,
        path: OwnedPath,
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

    pub fn unmount(&mut self, path: &Path) -> bool {
        self.mounts.remove(path).is_some()
    }

    pub fn open(path: impl AsRef<Path>, flags: OpenFlags) -> FsResult<FileRef> {
        fn open(path: &Path, flags: OpenFlags) -> FsResult<FileRef> {
            let Some((dev, path)) = VFS
                .lock()
                .mounts
                .iter()
                .rev()
                .find_map(|(mount, dev)| {
                    let rest = path.strip_prefix(mount)?;
                    Some((dev.clone(), rest))
                })
            else {
                return Err(FsError::PathNotFound);
            };

            Ok(unsafe { FileRef::new(dev.open(path, flags)?, dev) })
        }

        open(path.as_ref(), flags)
    }
}

pub static VFS: SpinLocked<Vfs> = SpinLocked::new(Vfs::new());
