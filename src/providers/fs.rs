use std::cmp::Ordering;
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use lscolors::{LsColors, Style};

use super::Error;
use crate::fisovec::FilterSorter;
use crate::tree::Provider;

pub struct Fs {
    root: PathBuf,
}

#[derive(PartialEq)]
enum FsNodeKind {
    Directory,
    SymLink(Option<PathBuf>),
    NamedPipe,
    CharDevice,
    BlockDevice,
    Regular,
    Socket,
    Executable,
}

use FsNodeKind::*;

pub struct FsNode {
    kind: FsNodeKind,
    name: String,
    meta: Option<Box<Metadata>>,
}

impl PartialEq for FsNode {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

#[cfg(unix)]
impl From<(PathBuf, &Option<Metadata>)> for FsNodeKind {
    fn from(value: (PathBuf, &Option<Metadata>)) -> Self {
        let Some(meta) = value.1 else {
            return Regular;
        };

        if meta.is_dir() {
            Directory
        } else if meta.is_symlink() {
            SymLink(fs::read_link(value.0).ok())
        } else {
            use std::os::unix::fs::FileTypeExt;
            use std::os::unix::fs::PermissionsExt;

            let ft = meta.file_type();
            if ft.is_fifo() {
                NamedPipe
            } else if ft.is_char_device() {
                CharDevice
            } else if ft.is_block_device() {
                BlockDevice
            } else if ft.is_socket() {
                Socket
            } else if meta.permissions().mode() & 0o111 != 0 {
                Executable
            } else {
                Regular
            }
        }
    }
}

#[cfg(windows)]
impl From<(PathBuf, &Option<Metadata>)> for FsNodeKind {
    fn from(value: (PathBuf, &Option<Metadata>)) -> Self {
        match value.0.extension() {
            Some(name)
                if [".exe", ".bat", ".cmd", ".com"]
                    .iter()
                    .any(|ext| *ext == name) =>
            {
                Executable
            }
            _ => Regular,
        }
    }
}

static LS_COLORS: OnceLock<LsColors> = OnceLock::new();

impl Display for FsNode {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        write!(
            f,
            "{}{}",
            LS_COLORS
                .get_or_init(|| LsColors::from_env().unwrap_or_default())
                .style_for_path_with_metadata(&self.name, self.meta.as_ref().map(Box::as_ref))
                .map(Style::to_ansi_term_style)
                .unwrap_or_default()
                .paint(&self.name),
            match self.kind {
                Directory => "/",
                SymLink(_) => "@",
                NamedPipe => "|",
                CharDevice | BlockDevice | Regular => "",
                Socket => "=",
                Executable => "*",
            }
        )?;

        if let SymLink(Some(path)) = &self.kind {
            write!(f, " -> {}", path.display())?;
        }

        Ok(())
    }
}

impl Provider for Fs {
    type Fragment = FsNode;

    fn provide_root(&self) -> Self::Fragment {
        FsNode {
            kind: Directory,
            name: self.root.to_string_lossy().into(),
            meta: fs::metadata(&self.root).ok().map(Box::new),
        }
    }

    fn provide(&mut self, path: Vec<&Self::Fragment>) -> Vec<Self::Fragment> {
        let Ok(dir) = fs::read_dir(path.into_iter().map(|n| &n.name).collect::<PathBuf>()) else {
            return Vec::new();
        };
        dir.filter_map(|d| {
            let entry = d.ok()?;
            let meta = entry.metadata().ok();
            let name = entry.file_name().into_string().ok()?;
            Some(FsNode {
                kind: (entry.path(), &meta).into(),
                name,
                meta: meta.map(Box::new),
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
