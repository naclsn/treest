use std::cmp::Ordering;

use anyhow::Result;
use thiserror::Error;

pub mod fs;

use crate::tree::NodePath;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fragment(pub usize);

/// A type that is able to provide a tree structure.
pub trait Provider {
    fn provide(&mut self, path: NodePath) -> Vec<Fragment>;

    fn order(&self, left: NodePath, right: NodePath) -> Ordering;
    fn keep(&self, path: NodePath) -> bool;

    fn display(&self, path: NodePath) -> String;
    fn breadcrumb(&self, path: NodePath) -> String {
        let mut v: Vec<String> = (0..path.head.len())
            .map(|k| {
                self.display(NodePath {
                    head: &path.head[..k],
                    tail: path.head[k],
                })
            })
            .collect();
        v.push(self.display(path));
        v.join("/")
    }
}

pub const NAMES: &'static [&'static str] = &["fs"];

pub fn select(arg: &str, name: Option<&str>) -> Result<Box<dyn Provider>> {
    fs::Fs::new(arg).map(|p| {
        let p: Box<dyn Provider> = Box::new(p);
        p
    })
}

/*

#[derive(Error, Debug)]
pub enum DynProviderError {
    #[error("the provider to use could not be guessed from the argument (see '--list')")]
    ProviderNeeded,
    #[error("'{0}' does not name an existing provider (see '--list')")]
    NotProvider(String),
}

macro_rules! providers {
    ($($nm:ident: $ty:ident if $ft:expr,)+) => {
        mod generic;
        $(pub mod $nm;)+

        pub const NAMES: &'static [&'static str] = &[$(stringify!($nm),)+];

        pub fn guess(arg: &str) -> Option<&'static str> {
            $(if $ft(arg) {
                return Some(stringify!($nm));
            })+
            None
        }

        pub fn select(arg: &str, name: Option<&str>) -> Result<dyn Provider> {
            let name = name.or_else(|| guess(arg)).ok_or(DynProviderError::ProviderNeeded)?;
            match name {
                $(stringify!($nm) => Ok(DynProvider::$ty($nm::$ty::new(arg)?)),)+
                _ => Err(DynProviderError::NotProvider(name.into()).into()),
            }
        }
    }
}

providers! {
    fs: Fs         if |path| std::path::Path::new(path).is_dir(),
    json: Json     if |path: &str| path.ends_with(".json"),
    proc: Proc     if |_| false,
    sqlite: Sqlite if |path: &str| [".sqlite", ".sqlite3", ".db"].iter().any(|&ext| path.ends_with(ext)),
    toml: Toml     if |path: &str| path.ends_with(".toml"),
    xml: Xml       if |path: &str| [".xml", ".htm", ".html"].iter().any(|&ext| path.ends_with(ext)),
    yaml: Yaml     if |path: &str| [".yaml", ".yml"].iter().any(|&ext| path.ends_with(ext)),
}
*/
