use std::cmp::Ordering;
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::marker::PhantomPinned;
use std::pin::Pin;
use std::ptr::NonNull;

use crate::fisovec::FilterSorter;
use crate::tree::{Provider, ProviderExt};

pub trait GenericValue {
    /// Operations that could be perform of some of the values, and which would lead to more,
    /// children, values; for example indexing by an number or a key. Default should be
    /// `Root`/`Top`/.. or similar to represent 'getting the tree root'.
    type Index: Default + PartialOrd;

    fn children(&self) -> Vec<(Self::Index, &Self)>;
    fn fmt_leaf(&self, f: &mut Formatter<'_>) -> FmtResult;
}

pub trait Generic {
    /// Value is, for example, the result of parsing. This is usually an enum of the various
    /// traditional primitive and generic types (say Number, String, Map, Array, ..).
    type Value: GenericValue;

    fn root(&self) -> &Pin<Box<(Self::Value, PhantomPinned)>>;
}

pub struct GenericFragment<T: Generic>(<T::Value as GenericValue>::Index, NonNull<T::Value>);

impl<T: Generic> PartialEq for GenericFragment<T> {
    fn eq(&self, other: &Self) -> bool {
        PartialEq::eq(&self.0, &other.0)
    }
}

impl<T: Generic> Provider for T {
    type Fragment = GenericFragment<T>;

    fn provide_root(&self) -> Self::Fragment {
        GenericFragment(
            <T::Value as GenericValue>::Index::default(),
            NonNull::from(&self.root().0),
        )
    }

    fn provide(&mut self, path: &[&Self::Fragment]) -> Vec<Self::Fragment> {
        unsafe { path.last().unwrap().1.as_ref() }
            .children()
            .into_iter()
            .map(|(i, v)| GenericFragment(i, v.into()))
            .collect()
    }
}

impl<T: Generic> Display for GenericFragment<T>
where
    <T::Value as GenericValue>::Index: Display,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "\x1b[34m{}\x1b[m\x1b[37m: ", self.0)?;
        unsafe { self.1.as_ref() }.fmt_leaf(f)
    }
}

impl<T: Generic> ProviderExt for T
where
    <T::Value as GenericValue>::Index: Display,
{
    fn fmt_frag_path(&self, f: &mut Formatter, path: &[&Self::Fragment]) -> FmtResult {
        path.iter()
            .try_for_each(|GenericFragment(i, _)| write!(f, " {i}"))
    }
}

impl<T: Generic> FilterSorter<<Self as Provider>::Fragment> for T {
    fn compare(
        &self,
        a: &<Self as Provider>::Fragment,
        b: &<Self as Provider>::Fragment,
    ) -> Option<Ordering> {
        PartialOrd::partial_cmp(&a.0, &b.0)
    }

    fn keep(&self, _a: &<Self as Provider>::Fragment) -> bool {
        // TODO
        true
    }
}
