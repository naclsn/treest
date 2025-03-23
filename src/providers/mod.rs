use std::cmp::Ordering;

use anyhow::Result;

use crate::tree::NodePath;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fragment(pub usize);

/// A type that is able to provide a tree structure.
pub trait Provider {
    fn provide(&mut self, path: &NodePath) -> Vec<Fragment>;

    fn order(&self, left: &NodePath, right: &NodePath) -> Ordering;
    fn keep(&self, path: &NodePath) -> bool;

    fn display(&self, path: &NodePath) -> String;
    fn components(&self, path: &NodePath) -> Vec<String> {
        let mut r: Vec<_> = (0..path.head.len())
            .map(|k| self.display(&path.head[..=k].into()))
            .collect();
        r.push(self.display(path));
        r
    }
    fn breadcrumbs(&self, path: &NodePath) -> String {
        self.components(path).join(" ")
    }
}

macro_rules! providers {
    ($($nm:ident: $ty:ident if $ft:expr,)+) => {
        //pub mod generic;
        $(pub mod $nm;)+

        pub const NAMES: &'static [&'static str] = &[$(stringify!($nm),)+];

        pub fn guess(arg: &str) -> Option<&'static str> {
            $(if $ft(arg) {
                return Some(stringify!($nm));
            })+
            None
        }

        pub fn select(arg: &str, name: &str) -> Result<Box<dyn Provider>> {
            match name {
                $(stringify!($nm) => $nm::$ty::new(arg).map(|p| {
                    let p: Box<dyn Provider> = Box::new(p);
                    p
                }),)+
                _ => unreachable!(),
            }
        }
    }
}

providers! {
    fs: Fs         if |path| std::path::Path::new(path).is_dir(),
    //json: Json     if |path: &str| path.ends_with(".json"),
    //proc: Proc     if |_| false,
    //sqlite: Sqlite if |path: &str| [".sqlite", ".sqlite3", ".db"].iter().any(|&ext| path.ends_with(ext)),
    //toml: Toml     if |path: &str| path.ends_with(".toml"),
    //xml: Xml       if |path: &str| [".xml", ".htm", ".html"].iter().any(|&ext| path.ends_with(ext)),
    //yaml: Yaml     if |path: &str| [".yaml", ".yml"].iter().any(|&ext| path.ends_with(ext)),
}
