use std::iter::FilterMap;
use std::ops::{Index, IndexMut};

#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct StabVec<T> {
    slots: Vec<Option<T>>,
    free_slots: usize,
}

#[macro_export]
macro_rules! stab_vec {
    ($($t:tt)*) => {
        $crate::stabvec::StabVec::from_elems([$($t)*])
    }
}

impl<T> StabVec<T> {
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            free_slots: 0,
        }
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            slots: Vec::with_capacity(cap),
            free_slots: 0,
        }
    }

    pub fn from_elems(elems: impl IntoIterator<Item = T>) -> Self {
        Self {
            slots: elems.into_iter().map(Some).collect(),
            free_slots: 0,
        }
    }

    pub fn insert(&mut self, it: T) -> usize {
        if let Some((k, o)) = match self.free_slots {
            0 => None,
            _ => self.slots.iter_mut().enumerate().find(|p| p.1.is_none()),
        } {
            *o = Some(it);
            self.free_slots -= 1;
            k
        } else {
            self.slots.push(Some(it));
            self.slots.len() - 1
        }
    }

    pub fn remove(&mut self, k: usize) -> Option<T> {
        let r = self.slots[k].take();
        self.free_slots += 1;
        while let Some(None) = self.slots.last() {
            self.slots.pop();
            self.free_slots -= 1;
        }
        r
    }

    pub fn replace(&mut self, k: usize, it: T) -> Option<T> {
        self.slots[k].replace(it)
    }

    pub fn iter(
        &self,
    ) -> FilterMap<<&Vec<Option<T>> as IntoIterator>::IntoIter, fn(&Option<T>) -> Option<&T>> {
        self.slots.iter().filter_map(Option::as_ref)
    }

    pub fn iter_mut(
        &mut self,
    ) -> FilterMap<
        <&mut Vec<Option<T>> as IntoIterator>::IntoIter,
        fn(&mut Option<T>) -> Option<&mut T>,
    > {
        self.slots.iter_mut().filter_map(Option::as_mut)
    }

    pub fn iter_ref(&self) -> impl Iterator<Item = (usize, &T)> + DoubleEndedIterator {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|p| Some(p.0).zip(p.1.as_ref()))
    }
}

impl<T> Index<usize> for StabVec<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        self.slots[index].as_ref().unwrap()
    }
}

impl<T> IndexMut<usize> for StabVec<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.slots[index].as_mut().unwrap()
    }
}

impl<'a, T> IntoIterator for &'a StabVec<T> {
    type Item = &'a T;
    type IntoIter =
        FilterMap<<&'a Vec<Option<T>> as IntoIterator>::IntoIter, fn(&Option<T>) -> Option<&T>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut StabVec<T> {
    type Item = &'a mut T;
    type IntoIter = FilterMap<
        <&'a mut Vec<Option<T>> as IntoIterator>::IntoIter,
        fn(&mut Option<T>) -> Option<&mut T>,
    >;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}
