use std::cmp::Ordering;
use std::fmt::{Display, Formatter, Result as FmtResult, Write};
use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::Result;
use lscolors::{LsColors, Style};
use thiserror::Error;

use crate::providers::{Fragment, Provider};
use crate::tree::NodePath;

pub struct Fs {
    //nodes: Vec<Node>,
    fs_nodes: Vec<FsNode>,
}

#[derive(Error, Debug)]
pub enum FsProviderError {
    #[error("path is not a directory")]
    NotADirectory,
}

#[derive(PartialEq)]
enum FsNodeKind {
    Directory,
    SymLink(Option<PathBuf>), // FIXME: broken
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

// platform-dep {{{
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

#[inline(always)]
fn write_perm(f: &mut impl Write, perm: u32) -> FmtResult {
    write!(
        f,
        "{}{}{}",
        if (perm >> 2) & 0b1 == 1 { 'r' } else { '-' },
        if (perm >> 1) & 0b1 == 1 { 'w' } else { '-' },
        if perm & 0b1 == 1 { 'x' } else { '-' },
    )
}

#[cfg(unix)]
fn write_meta(f: &mut impl Write, node: &FsNode) -> FmtResult {
    use std::os::unix::fs::PermissionsExt;
    let mode = node
        .meta
        .as_ref()
        .map(|m| m.permissions().mode())
        .unwrap_or(0);

    write!(
        f,
        "{}",
        match node.kind {
            Directory => 'd',
            SymLink(_) => 'l',
            NamedPipe => 'p',
            CharDevice => 'c',
            BlockDevice => 'b',
            Socket => 's',
            Regular | Executable => '-',
        }
    )?;
    // owner
    write_perm(f, (mode >> 6) & 0b111)?;
    // group
    write_perm(f, (mode >> 3) & 0b111)?;
    // world
    write_perm(f, mode & 0b111)
}

#[cfg(windows)]
fn write_meta(f: &mut impl Write, node: &FsNode) -> FmtResult {
    let ro = node
        .meta
        .as_ref()
        .map(|m| m.permissions().readonly())
        .unwrap_or(true);

    write!(
        f,
        "{}",
        match node.kind {
            Directory => 'd',
            SymLink(_) => 'l',
            _ => '-',
        }
    )?;
    // owner
    write_perm(f, 0b101 | if ro { 0b000 } else { 0b010 })?;
    // group
    write_perm(f, 0b101 | if ro { 0b000 } else { 0b010 })?;
    // world
    write_perm(f, 0b101 | if ro { 0b000 } else { 0b010 })
}
// }}}

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
    //fn tree(&self) -> &Tree { &self.nodes }
    //fn tree_mut(&mut self) -> &mut Vec<Node> { &mut self.nodes }

    fn provide(&mut self, path: &NodePath) -> Vec<Fragment> {
        let mut pb = path
            .head
            .iter()
            .map(|n| &self.fs_nodes[n.fragment.0].name)
            .collect::<PathBuf>();
        pb.push(&self.fs_nodes[path.tail.fragment.0].name);

        let Ok(dir) = fs::read_dir(pb) else {
            return Vec::new();
        };

        dir.filter_map(|d| {
            let entry = d.ok()?;
            let meta = entry.metadata().ok();
            let name = entry.file_name().into_string().ok()?;

            self.fs_nodes.push(FsNode {
                kind: (entry.path(), &meta).into(),
                name,
                meta: meta.map(Box::new),
            });

            Some(Fragment(self.fs_nodes.len() - 1))
        })
        .collect()
    }

    fn order(&self, left: &NodePath, right: &NodePath) -> Ordering {
        let left = &self.fs_nodes[left.tail.fragment.0];
        let right = &self.fs_nodes[right.tail.fragment.0];
        Ord::cmp(&left.name, &right.name)
    }

    fn keep(&self, path: &NodePath) -> bool {
        let node = &self.fs_nodes[path.tail.fragment.0];
        !node.name.starts_with('.')
    }

    fn display(&self, path: &NodePath) -> String {
        let node = &self.fs_nodes[path.tail.fragment.0];
        node.to_string()
    }
}

impl Fs {
    pub fn new(root: impl AsRef<Path>) -> Result<Self> {
        let mut root = PathBuf::from(root.as_ref());
        if root.components().next().is_none() {
            root.push(".");
        }
        if !root.is_dir() {
            Err(FsProviderError::NotADirectory.into())
        } else {
            Ok(Self {
                //nodes: vec![Node::default()],
                fs_nodes: vec![FsNode {
                    kind: Directory,
                    name: root.to_string_lossy().into(),
                    meta: root.metadata().ok().map(Box::new), //meta: fs::metadata(root).ok().map(Box::new),
                }],
            })
        }
    }
}
