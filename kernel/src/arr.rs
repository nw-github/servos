use core::ops::{Deref, DerefMut, Index, IndexMut};

pub struct HoleArray<T, const N: usize>(pub [Option<T>; N]);

impl<const N: usize, T> HoleArray<T, N> {
    pub const fn new(data: [Option<T>; N]) -> Self {
        Self(data)
    }

    pub const fn empty() -> Self {
        Self([const { None }; N])
    }

    pub fn find_free_space(&mut self) -> Option<(usize, &mut Option<T>)> {
        self.iter_mut().enumerate().find(|elem| elem.1.is_none())
    }

    pub fn push(&mut self, item: T) -> Result<(usize, &mut T), T> {
        let Some((i, elem)) = self.find_free_space() else {
            return Err(item);
        };
        Ok((i, elem.insert(item)))
    }

    pub fn remove(&mut self, i: usize) -> Option<T> {
        self.0.get_mut(i).and_then(|v| v.take())
    }

    pub fn get(&self, i: usize) -> Option<&T> {
        self.0.get(i).and_then(|v| v.as_ref())
    }
}

impl<const N: usize, T> Deref for HoleArray<T, N> {
    type Target = [Option<T>; N];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<const N: usize, T> DerefMut for HoleArray<T, N> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<const N: usize, T> Index<usize> for HoleArray<T, N> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        self.0[index].as_ref().unwrap()
    }
}

impl<const N: usize, T> IndexMut<usize> for HoleArray<T, N> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.0[index].as_mut().unwrap()
    }
}
