use std::cmp::Ordering;
use std::ops::{Index, IndexMut};

pub trait FilterSorter<T> {
    fn compare(&self, a: &T, b: &T) -> Ordering;
    fn keep(&self, a: &T) -> bool;
}

#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FisoVec<T> {
    inner: Vec<T>,
    indices: Vec<usize>,
}

impl<T> FisoVec<T> {
    pub fn filter_sort(&mut self, with: &impl FilterSorter<T>) {
        self.indices = self
            .inner
            .iter()
            .enumerate()
            .filter_map(|v| if with.keep(v.1) { Some(v.0) } else { None })
            .collect();
        self.indices
            .sort_unstable_by(|a, b| with.compare(&self.inner[*a], &self.inner[*b]));
    }

    pub fn len(&self) -> usize {
        self.indices.len()
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &T> + ExactSizeIterator {
        self.indices.iter().map(|k| &self.inner[*k])
    }

    pub fn into_inner(self) -> Vec<T> {
        self.inner
    }

    // :<
    pub(crate) fn inner_remove(&mut self, k: usize) -> T {
        self.inner.remove(k)
    }
}

impl<T> FromIterator<T> for FisoVec<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let inner: Vec<_> = iter.into_iter().collect();
        let indices = (0..inner.len()).collect();
        Self { inner, indices }
    }
}

impl<T> Index<usize> for FisoVec<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        &self.inner[self.indices[index]]
    }
}

impl<T> IndexMut<usize> for FisoVec<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.inner[self.indices[index]]
    }
}

impl<T> AsRef<[T]> for FisoVec<T> {
    fn as_ref(&self) -> &[T] {
        self.inner.as_ref()
    }
}

impl<T> AsMut<[T]> for FisoVec<T> {
    fn as_mut(&mut self) -> &mut [T] {
        self.inner.as_mut()
    }
}
