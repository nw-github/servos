use core::mem::MaybeUninit;

use alloc::{sync::Arc, vec::Vec};
use shared::io::{DirEntry, OpenFlags, Stat};

use crate::dev::Device;

use super::{
    path::{OwnedPath, Path},
    vfs::MountError,
    FileSystem, FsError, FsResult, VNode,
};

pub struct DeviceFs {
    devices: Vec<(OwnedPath, Arc<dyn Device>)>,
}

impl DeviceFs {
    pub const fn new() -> Self {
        Self {
            devices: Vec::new(),
        }
    }

    pub fn add_device(&mut self, name: OwnedPath, dev: Arc<dyn Device>) -> Result<(), MountError> {
        assert!(name.components().count() == 1);
        if self.find_device(&name).is_some() {
            return Err(MountError::AlreadyMounted);
        }

        if self.devices.try_reserve(1).is_err() {
            return Err(MountError::NoMem);
        }

        self.devices.push((name, dev));
        Ok(())
    }

    fn find_device(&self, name: &Path) -> Option<usize> {
        self.devices.iter().position(|(dev, _)| dev == name)
    }
}

impl FileSystem for DeviceFs {
    fn open(&self, path: &Path, flags: OpenFlags, root: Option<&VNode>) -> FsResult<VNode> {
        if !path.is_absolute() && root.is_some_and(|r| !r.directory) {
            return Err(FsError::PathNotFound);
        }

        let mut components = path.components();
        if components.next().is_none() {
            return Ok(VNode {
                ino: 0,
                directory: true,
                readonly: true,
            });
        } else if components.next().is_some() {
            return Err(FsError::PathNotFound);
        }

        let Some(ino) = self.find_device(path) else {
            return Err(FsError::PathNotFound);
        };
        // if devices become removable this will have to change
        Ok(VNode {
            ino: ino as u64,
            directory: false,
            readonly: !flags.contains(OpenFlags::ReadWrite),
        })
    }

    fn read<'a>(
        &self,
        vn: &VNode,
        pos: u64,
        buf: &'a mut [MaybeUninit<u8>],
    ) -> FsResult<&'a mut [u8]> {
        self.devices[vn.ino as usize].1.read(pos, buf)
    }

    fn write(&self, vn: &VNode, pos: u64, buf: &[u8]) -> FsResult<usize> {
        self.devices[vn.ino as usize].1.write(pos, buf)
    }

    fn close(&self, _vn: &VNode) -> FsResult<()> {
        Ok(())
    }

    fn readdir(&self, vn: &VNode, pos: usize) -> FsResult<Option<DirEntry>> {
        if !vn.directory {
            return Err(FsError::InvalidOp);
        }

        if let Some((name, _)) = self.devices.get(pos) {
            let name: &[u8] = name.as_ref().as_ref();
            let mut dir = DirEntry {
                name: [0; 256],
                name_len: name.len(),
                stat: Stat {
                    directory: false,
                    size: 0,
                    readonly: false,
                },
            };
            dir.name[..name.len()].copy_from_slice(name);
            Ok(Some(dir))
        } else {
            Ok(None)
        }
    }

    fn stat(&self, vn: &VNode) -> FsResult<Stat> {
        if vn.directory {
            Ok(Stat {
                directory: true,
                size: 0,
                readonly: true,
            })
        } else {
            Ok(Stat {
                directory: false,
                size: 0,
                readonly: false,
            })
        }
    }
}

unsafe impl Sync for DeviceFs {}
unsafe impl Send for DeviceFs {}
