use std::fmt::{Display, Formatter, Result as FmtResult};
use std::io::Error as IoError;
use std::path::PathBuf;

use crate::fisovec::FilterSorter;
use crate::tree::Provider;

#[derive(Debug)]
pub enum Error {
    NotProvider(String),
    NotDirectory(PathBuf),
    IoErr(IoError),
    ParseErr(usize),
}

macro_rules! providers {
    ($($nm:ident: $ty:ident,)+) => {
        $(pub mod $nm;)+

        pub enum DynProvider {
            $($ty($nm::$ty),)+
        }

        #[derive(PartialEq)]
        pub enum DynFragment {
            $($ty(<$nm::$ty as Provider>::Fragment),)+
        }

        impl Display for DynFragment {
            fn fmt(&self, f: &mut Formatter) -> FmtResult {
                match self {
                    $(DynFragment::$ty(it) => it.fmt(f),)+
                }
            }
        }

        impl DynFragment {
            $(fn $nm(&self) -> &<$nm::$ty as Provider>::Fragment {
                match self {
                    DynFragment::$ty(it) => it,
                    _ => unreachable!(),
                }
            })+
        }

        impl Provider for DynProvider {
            type Fragment = DynFragment;

            fn provide_root(&self) -> Self::Fragment {
                match self {
                    $(DynProvider::$ty(it) => DynFragment::$ty(it.provide_root()),)+
                }
            }

            fn provide(&mut self, path: Vec<&Self::Fragment>) -> Vec<Self::Fragment> {
                match self {
                    $(DynProvider::$ty(it) => it
                        .provide(path.into_iter().map(DynFragment::$nm).collect())
                        .into_iter()
                        .map(DynFragment::$ty)
                        .collect(),)+
                }
            }
        }

        impl FilterSorter<DynFragment> for DynProvider {
            fn compare(&self, a: &DynFragment, b: &DynFragment) -> std::cmp::Ordering {
                match self {
                    $(DynProvider::$ty(it) => it.compare(a.$nm(), b.$nm()),)+
                }
            }

            fn keep(&self, a: &DynFragment) -> bool {
                match self {
                    $(DynProvider::$ty(it) => it.keep(a.$nm()),)+
                }
            }
        }

        pub fn select(name: &str, arg: &str) -> Result<DynProvider, Error> {
            match name {
                $(stringify!($nm) => Ok(DynProvider::$ty($nm::$ty::new(arg)?)),)+
                _ => Err(Error::NotProvider(name.into())),
            }
        }
    };
}

providers! {
    fs: Fs,
    json: Json,
}
