use std::cmp::Ordering;
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::fs;
use std::path::{Path, PathBuf};

use super::Error;
use crate::fisovec::FilterSorter;
use crate::tree::Provider;

pub struct Fs {
    root: PathBuf,
}

#[derive(PartialEq)]
enum FsNodeKind {
    Dir,
    Exec,
    File,
}

#[derive(PartialEq)]
pub struct FsNode {
    kind: FsNodeKind,
    name: String,
}

impl Display for FsNode {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        write!(
            f,
            "{}{}{}",
            match self.kind {
                FsNodeKind::Dir => "\x1b[34m",
                FsNodeKind::Exec => "\x1b[32m",
                FsNodeKind::File => "",
            },
            self.name,
            match self.kind {
                FsNodeKind::Dir => "\x1b[m/",
                FsNodeKind::Exec => "\x1b[m*",
                FsNodeKind::File => "",
            }
        )
    }
}

impl Provider for Fs {
    type Fragment = FsNode;

    fn provide_root(&self) -> Self::Fragment {
        FsNode {
            kind: FsNodeKind::Dir,
            name: self.root.to_string_lossy().into(),
        }
    }

    fn provide(&mut self, path: Vec<&Self::Fragment>) -> Vec<Self::Fragment> {
        let Ok(dir) = fs::read_dir(path.into_iter().map(|n| &n.name).collect::<PathBuf>()) else {
            return Vec::new();
        };
        dir.filter_map(|d| {
            let entry = d.ok()?;
            let meta = entry.metadata().ok()?;
            let name = entry.file_name().into_string().ok()?;
            Some(FsNode {
                kind: if meta.is_dir() {
                    FsNodeKind::Dir
                //} else if m.is_symlink() {
                //    FsNodeKind::Link
                } else if [".exe", ".bat", ".cmd", ".com"]
                    .iter()
                    .any(|ext| name.ends_with(ext))
                {
                    FsNodeKind::Exec
                } else {
                    FsNodeKind::File
                },
                name,
            })
        })
        .collect()
    }
}

impl FilterSorter<FsNode> for Fs {
    fn compare(&self, a: &FsNode, b: &FsNode) -> Ordering {
        Ord::cmp(&a.name, &b.name)
    }

    fn keep(&self, a: &FsNode) -> bool {
        !a.name.starts_with('.')
    }
}

impl Fs {
    pub fn new(root: impl AsRef<Path>) -> Result<Self, Error> {
        let mut root = PathBuf::from(root.as_ref());
        if root.components().next().is_none() {
            root.push(".");
        }
        if !root.is_dir() {
            Err(Error::NotDirectory(root))
        } else {
            Ok(Self {
                root: root.components().collect(),
            })
        }
    }
}
