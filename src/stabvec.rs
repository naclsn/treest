use std::ops::{Index, IndexMut};

#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StabVec<T> {
    slots: Vec<Option<T>>,
    free_slots: usize,
}

#[allow(dead_code)]
impl<T> StabVec<T> {
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

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.slots.iter().filter_map(Option::as_ref)
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.slots.iter_mut().filter_map(Option::as_mut)
    }

    pub fn iter_ref(&self) -> impl DoubleEndedIterator<Item = (usize, &T)> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|p| Some(p.0).zip(p.1.as_ref()))
    }
}

impl<T> FromIterator<T> for StabVec<T> {
    fn from_iter<I: IntoIterator<Item = T>>(value: I) -> Self {
        Self {
            slots: value.into_iter().map(Some).collect(),
            free_slots: 0,
        }
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
