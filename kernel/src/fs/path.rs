use core::{
    borrow::Borrow,
    ops::{Deref, Range},
};

use alloc::{boxed::Box, collections::TryReserveError, vec::Vec};

#[repr(transparent)]
#[derive(Eq)]
pub struct Path([u8]);

impl Path {
    pub fn new<'a>(path: impl AsRef<[u8]> + 'a) -> &'a Path {
        unsafe { core::mem::transmute(path.as_ref()) }
    }

    pub fn components(&self) -> impl Iterator<Item = &[u8]> + Clone {
        self.0.split(|&c| c == b'/').filter(|c| !c.is_empty())
    }

    pub fn starts_with(&self, rhs: impl AsRef<Path>) -> bool {
        iter_after(self.components(), rhs.as_ref().components()).is_some()
    }

    pub fn strip_prefix(&self, rhs: impl AsRef<Path>) -> Option<&Path> {
        iter_after(self.components(), rhs.as_ref().components()).map(|mut c| {
            // TODO: fix this incredibly stupid hack
            let Some(next) = c.next() else {
                return Path::new("");
            };
            Path::new(unsafe {
                core::slice::from_ptr_range(Range {
                    start: next.as_ptr(),
                    end: c.last().unwrap_or(next).as_ptr_range().end,
                })
            })
        })
    }

    pub fn is_empty(&self) -> bool {
        self.components().next().is_none()
    }
}

impl PartialEq for Path {
    fn eq(&self, rhs: &Self) -> bool {
        // ensure both paths are absolute or relative. this is probably not very robust
        if self.0.first() != rhs.0.first() {
            return false;
        }

        self.components().eq(rhs.components())
    }
}

impl PartialOrd for Path {
    fn partial_cmp(&self, rhs: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(rhs))
    }
}

impl Ord for Path {
    fn cmp(&self, rhs: &Self) -> core::cmp::Ordering {
        let first = self.0.first().cmp(&rhs.0.first());
        if !matches!(first, core::cmp::Ordering::Equal) {
            return first;
        }

        self.components().cmp(rhs.components())
    }
}

impl AsRef<[u8]> for Path {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl AsRef<Path> for str {
    fn as_ref(&self) -> &Path {
        self.as_bytes().as_ref()
    }
}

impl AsRef<Path> for [u8] {
    fn as_ref(&self) -> &Path {
        Path::new(self)
    }
}

impl AsRef<Path> for &Path {
    fn as_ref(&self) -> &Path {
        Path::new(self)
    }
}

impl<const N: usize> AsRef<Path> for [u8; N] {
    fn as_ref(&self) -> &Path {
        self[..].as_ref()
    }
}

impl<'a, T: AsRef<[u8]>> From<&'a T> for &'a Path {
    fn from(value: &'a T) -> Self {
        Path::new(value.as_ref())
    }
}

impl TryInto<OwnedPath> for &Path {
    type Error = TryReserveError;

    fn try_into(self) -> Result<OwnedPath, Self::Error> {
        let mut v = Vec::try_with_capacity(self.0.len())?;
        v.extend_from_slice(self.as_ref());
        Ok(OwnedPath(v.into()))
    }
}

#[repr(transparent)]
#[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OwnedPath(Box<[u8]>);

impl AsRef<Path> for OwnedPath {
    fn as_ref(&self) -> &Path {
        Path::new(&self.0)
    }
}

impl Deref for OwnedPath {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl Borrow<Path> for OwnedPath {
    fn borrow(&self) -> &Path {
        self.as_ref()
    }
}

impl PartialEq<Path> for OwnedPath {
    fn eq(&self, other: &Path) -> bool {
        self.as_ref() == other
    }
}

// stolen from rust stdlib :P
fn iter_after<'a, 'b, I, J>(mut iter: I, mut prefix: J) -> Option<I>
where
    I: Iterator<Item = &'a [u8]> + Clone,
    J: Iterator<Item = &'b [u8]>,
{
    loop {
        let mut iter_next = iter.clone();
        match (iter_next.next(), prefix.next()) {
            (Some(ref x), Some(ref y)) if x == y => (),
            (Some(_), Some(_)) => return None,
            (Some(_), None) => return Some(iter),
            (None, None) => return Some(iter),
            (None, Some(_)) => return None,
        }
        iter = iter_next;
    }
}
