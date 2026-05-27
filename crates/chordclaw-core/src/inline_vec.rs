use std::ops::{Deref, DerefMut};
use std::slice;

use serde::{Serialize, Serializer};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) struct InlineVec<T, const N: usize> {
    len: u8,
    items: [T; N],
}

impl<T: Copy + Default, const N: usize> Default for InlineVec<T, N> {
    fn default() -> Self {
        Self {
            len: 0,
            items: [T::default(); N],
        }
    }
}

impl<T, const N: usize> InlineVec<T, N> {
    pub(crate) const fn from_parts(len: u8, items: [T; N]) -> Self {
        Self { len, items }
    }

    pub(crate) fn as_slice(&self) -> &[T] {
        &self.items[..usize::from(self.len)]
    }

    pub(crate) fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.items[..usize::from(self.len)]
    }

    pub(crate) const fn len(&self) -> usize {
        self.len as usize
    }

    pub(crate) const fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl<T: Copy, const N: usize> InlineVec<T, N> {
    pub(crate) fn push(&mut self, value: T) -> bool {
        if usize::from(self.len) == N {
            return false;
        }

        self.items[usize::from(self.len)] = value;
        self.len += 1;
        true
    }

    pub(crate) fn clear(&mut self) {
        self.len = 0;
    }

    pub(crate) fn retain(&mut self, mut keep: impl FnMut(&T) -> bool) {
        let mut out = 0;
        for idx in 0..usize::from(self.len) {
            let item = self.items[idx];
            if keep(&item) {
                self.items[out] = item;
                out += 1;
            }
        }
        self.len = out as u8;
    }
}

impl<T: Ord, const N: usize> InlineVec<T, N> {
    pub(crate) fn sort_unstable(&mut self) {
        self.as_mut_slice().sort_unstable();
    }
}

impl<T, const N: usize> InlineVec<T, N> {
    pub(crate) fn sort_unstable_by_key<K: Ord>(&mut self, key: impl FnMut(&T) -> K) {
        self.as_mut_slice().sort_unstable_by_key(key);
    }
}

impl<T: Copy + PartialEq, const N: usize> InlineVec<T, N> {
    pub(crate) fn dedup(&mut self) {
        if self.len <= 1 {
            return;
        }

        let mut out = 1usize;
        for idx in 1..usize::from(self.len) {
            let item = self.items[idx];
            if self.items[out - 1] == item {
                continue;
            }
            self.items[out] = item;
            out += 1;
        }
        self.len = out as u8;
    }
}

impl<T, const N: usize> Deref for InlineVec<T, N> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl<T, const N: usize> DerefMut for InlineVec<T, N> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut_slice()
    }
}

impl<'a, T, const N: usize> IntoIterator for &'a InlineVec<T, N> {
    type IntoIter = slice::Iter<'a, T>;
    type Item = &'a T;

    fn into_iter(self) -> Self::IntoIter {
        self.as_slice().iter()
    }
}

impl<T: Serialize, const N: usize> Serialize for InlineVec<T, N> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.as_slice().serialize(serializer)
    }
}
