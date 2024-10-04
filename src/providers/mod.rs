use std::error::Error;
use std::fmt::{Display, Formatter, Result as FmtResult};

use anyhow::Result;

#[derive(Debug)]
pub enum DynProviderError {
    ProviderNeeded,
    NotProvider(String),
}
impl Display for DynProviderError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        use DynProviderError::*;
        match self {
            ProviderNeeded => write!(f, "the provider to use could not be guessed from argument"),
            NotProvider(name) => write!(f, "'{name}' does not name an existing provider"),
        }
    }
}
impl Error for DynProviderError {}

macro_rules! providers {
    ($($nm:ident: $ty:ident if $ft:expr,)+) => {
        mod generic;
        $(pub mod $nm;)+

        pub const NAMES: &'static [&'static str] = &[$(stringify!($nm),)+];

        pub enum DynProvider {
            $($ty($nm::$ty),)+
        }

        #[derive(PartialEq)]
        pub enum DynFragment {
            $($ty(<$nm::$ty as $crate::tree::Provider>::Fragment),)+
        }

        impl std::fmt::Display for DynFragment {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                match self {
                    $(DynFragment::$ty(it) => it.fmt(f),)+
                }
            }
        }

        impl DynFragment {
            $(fn $nm(&self) -> &<$nm::$ty as $crate::tree::Provider>::Fragment {
                match self {
                    DynFragment::$ty(it) => it,
                    _ => unreachable!(),
                }
            })+
        }

        impl $crate::tree::Provider for DynProvider {
            type Fragment = DynFragment;

            fn provide_root(&self) -> Self::Fragment {
                match self {
                    $(DynProvider::$ty(it) => DynFragment::$ty(it.provide_root()),)+
                }
            }

            fn provide(&mut self, path: &[&Self::Fragment]) -> Vec<Self::Fragment> {
                match self {
                    $(DynProvider::$ty(it) => it
                        .provide(&path.iter().copied().map(DynFragment::$nm).collect::<Vec<_>>())
                        .into_iter()
                        .map(DynFragment::$ty)
                        .collect(),)+
                }
            }
        }

        impl $crate::tree::ProviderExt for DynProvider {
            fn write_nav_path(&self, f: &mut impl std::fmt::Write, path: &[&Self::Fragment]) -> std::fmt::Result {
                match self {
                    $(DynProvider::$ty(it) => it.write_nav_path(f, &path
                        .iter()
                        .copied()
                        .map(DynFragment::$nm)
                        .collect::<Vec<_>>()),)+
                }
            }

            fn write_arg_path(&self, f: &mut impl std::fmt::Write, path: &[&Self::Fragment]) -> std::fmt::Result {
                match self {
                    $(DynProvider::$ty(it) => it.write_arg_path(f, &path
                        .iter()
                        .copied()
                        .map(DynFragment::$nm)
                        .collect::<Vec<_>>()),)+
                }
            }

            fn command(&mut self, cmd: &[String]) -> Result<String> {
                match self {
                    $(DynProvider::$ty(it) => it.command(cmd),)+
                }
            }
        }

        impl $crate::fisovec::FilterSorter<DynFragment> for DynProvider {
            fn compare(&self, a: &DynFragment, b: &DynFragment) -> Option<std::cmp::Ordering> {
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

        pub fn guess(arg: &str) -> Option<&'static str> {
            $(if $ft(arg) {
                return Some(stringify!($nm));
            })+
            None
        }

        pub fn select(arg: &str, name: Option<&str>) -> Result<DynProvider> {
            let name = name.or_else(|| guess(arg)).ok_or(DynProviderError::ProviderNeeded)?;
            match name {
                $(stringify!($nm) => Ok(DynProvider::$ty($nm::$ty::new(arg)?)),)+
                _ => Err(DynProviderError::NotProvider(name.into()).into()),
            }
        }
    };
}

providers! {
    fs: Fs         if |path| std::path::Path::new(path).is_dir(),
    json: Json     if |ext: &str| ext.ends_with(".json"),
    sqlite: Sqlite if |ext: &str| [".sqlite", ".sqlite3", ".db"].iter().any(|&s| ext.ends_with(s)),
    toml: Toml     if |ext: &str| ext.ends_with(".toml"),
    yaml: Yaml     if |ext: &str| [".yaml", ".yml"].iter().any(|&s| ext.ends_with(s)),
}
