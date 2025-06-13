use std::cmp::Ordering;
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::Result;
use chrono::{DateTime, Local};
use lscolors::{LsColors, Style};
use thiserror::Error;

use crate::provider::{Event, EventKind, EventPoller, Provider};
use crate::tree::{Fragment, Node, NodePath};

pub struct Fs(PathBuf);

#[derive(Error, Debug)]
pub enum FsProviderError {
    #[error("path is not a directory")]
    NotADirectory,
}

#[derive(Debug, PartialEq)]
enum FsNodeKind {
    Directory(usize),
    SymLink(String, Box<FsNodeKind>),
    NamedPipe,
    CharDevice,
    BlockDevice,
    Regular,
    Socket,
    Executable,
}

use FsNodeKind::*;

#[derive(Debug)]
pub struct FsNode {
    kind: FsNodeKind,
    name: String,
    meta: Option<Metadata>,
}

// platform-dep {{{
#[cfg(unix)]
impl From<(PathBuf, &Option<Metadata>)> for FsNodeKind {
    fn from(value: (PathBuf, &Option<Metadata>)) -> Self {
        let Some(meta) = value.1 else { return Regular };

        if meta.is_dir() {
            Directory(value.0.read_dir().map(|ls| ls.count()).unwrap_or(0))
        } else if meta.is_symlink() {
            let target = fs::read_link(&value.0)
                .map(|t| t.to_string_lossy().to_string())
                .unwrap_or("?".to_string());

            let full_path = value.0.parent().unwrap().join(&target);
            let meta = full_path.metadata().ok();

            // TODO: protect from infinite recursion
            //   and while at it, don't use this `From<(,)>` weird idea
            SymLink(target, Box::new((full_path, &meta).into()))
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
        let Some(meta) = value.1 else { return Regular };

        if meta.is_dir() {
            Directory(value.0.read_dir().map(|ls| ls.count()).unwrap_or(0))
        } else {
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
}

#[inline]
fn write_perm(perm: u32) -> String {
    format!(
        "{}{}{}",
        if (perm >> 2) & 0b1 == 1 { 'r' } else { '-' },
        if (perm >> 1) & 0b1 == 1 { 'w' } else { '-' },
        if perm & 0b1 == 1 { 'x' } else { '-' },
    )
}

#[cfg(unix)]
fn write_meta(node: &FsNode) -> String {
    use std::os::unix::fs::PermissionsExt;
    let mode = node
        .meta
        .as_ref()
        .map(|m| m.permissions().mode())
        .unwrap_or(0);

    format!(
        "{}{}{}{}",
        match node.kind {
            Directory(_) => 'd',
            SymLink(_, _) => 'l',
            NamedPipe => 'p',
            CharDevice => 'c',
            BlockDevice => 'b',
            Socket => 's',
            Regular | Executable => '-',
        },
        write_perm((mode >> 6) & 0b111), // owner
        write_perm((mode >> 3) & 0b111), // group
        write_perm(mode & 0b111),        // world
    )
}

#[cfg(windows)]
fn write_meta(node: &FsNode) -> String {
    let ro = node
        .meta
        .as_ref()
        .map(|m| m.permissions().readonly())
        .unwrap_or(true);

    format!(
        "{}{}{}{}",
        match node.kind {
            Directory => 'd',
            SymLink(_, _) => 'l',
            _ => '-',
        },
        write_perm(0b101 | if ro { 0b000 } else { 0b010 }), // owner
        write_perm(0b101 | if ro { 0b000 } else { 0b010 }), // group
        write_perm(0b101 | if ro { 0b000 } else { 0b010 }), // world
    )
}
// }}}

static LS_COLORS: OnceLock<LsColors> = OnceLock::new();

impl FsNode {
    fn display(&self, full_path: &Path, show_child_count: bool) -> String {
        let ls_colors = LS_COLORS.get_or_init(|| LsColors::from_env().unwrap_or_default());

        let mut r = ls_colors
            .style_for_path_with_metadata(full_path, self.meta.as_ref())
            .map(Style::to_ansi_term_style)
            .unwrap_or_default()
            .paint(&self.name)
            .to_string();

        match &self.kind {
            Directory(count) => {
                r.push(std::path::MAIN_SEPARATOR);
                if show_child_count {
                    r += &format!(" \x1b[37m({count})");
                }
            }

            SymLink(target, kind) => {
                r += "@ -> ";
                r += &ls_colors
                    .style_for_path(full_path.parent().unwrap().join(target))
                    .map(Style::to_ansi_term_style)
                    .unwrap_or_default()
                    .paint(target)
                    .to_string();
                match &**kind {
                    Directory(_) => r.push(std::path::MAIN_SEPARATOR),
                    SymLink(_, _) => r.push('@'),
                    NamedPipe => r.push('|'),
                    CharDevice | BlockDevice | Regular => (),
                    Socket => r.push('='),
                    Executable => r.push('*'),
                }
            }

            NamedPipe => r.push('|'),
            CharDevice | BlockDevice | Regular => (),
            Socket => r.push('='),
            Executable => r.push('*'),
        };

        r
    }
}

impl Provider for Fs {
    fn provide_root(&self) -> Fragment {
        Box::new(FsNode {
            kind: Directory(self.0.read_dir().map(|ls| ls.count()).unwrap_or(0)),
            name: self.0.to_string_lossy().trim_end_matches(std::path::MAIN_SEPARATOR).to_string(),
            meta: self.0.metadata().ok(),
        })
    }

    fn provide(&mut self, path: &NodePath) -> Vec<Fragment> {
        let mut pb: PathBuf = path
            .head
            .iter()
            .map(|n| &n.fragment::<FsNode>().name)
            .collect();
        pb.push(&path.tail.fragment::<FsNode>().name);

        let Ok(dir) = fs::read_dir(pb) else {
            return Vec::new();
        };

        dir.filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name().into_string().ok()?;
                let meta = entry.metadata().ok();
                Some(Box::new(FsNode {
                    kind: (entry.path(), &meta).into(),
                    name,
                    meta,
                }) as _)
            })
            .collect()
    }

    fn order(&self, left: &NodePath, right: &NodePath) -> Ordering {
        let left: &FsNode = left.tail.fragment();
        let right: &FsNode = right.tail.fragment();
        Ord::cmp(&left.name, &right.name)
    }

    fn keep(&self, path: &NodePath) -> bool {
        let node: &FsNode = path.tail.fragment();
        !node.name.starts_with('.')
    }

    fn display(&self, path: &NodePath) -> String {
        let node: &FsNode = path.tail.fragment();
        let mut full_path: PathBuf = path
            .head
            .iter()
            .map(|it| &it.fragment::<FsNode>().name)
            .collect();
        full_path.push(&node.name);
        node.display(&full_path, true)
    }

    fn components(&self, path: &NodePath) -> Vec<String> {
        path.head
            .iter()
            .chain(std::iter::once(&path.tail))
            .map(|n| n.fragment::<FsNode>().name.clone())
            .collect()
    }

    fn join(&self, components: &[String]) -> String {
        components.join(std::path::MAIN_SEPARATOR_STR)
    }

    fn split<'a>(&self, path: &'a NodePath<'a>, text: String) -> Result<(Vec<&'a Node>, Fragment)> {
        todo!("split {path:?} {text:?}")
        //Ok((path.iter_all().collect(), Box::new(text)))
    }

    fn breadcrumbs(&self, path: &NodePath) -> String {
        let node: &FsNode = path.tail.fragment();
        let mut r = write_meta(node);

        match &node.meta {
            Some(meta) => {
                r.push_str(&format!(" {:8} ", meta.len()));
                match meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .and_then(|du| DateTime::from_timestamp(du.as_secs() as i64, du.subsec_nanos()))
                    .map(|dt| dt.with_timezone(&Local))
                {
                    Some(dt) => r.push_str(&dt.format("%b %e %H:%M ").to_string()),
                    None => r.push_str("??? ?? ??:?? "),
                }
            }
            None => r.push_str("        ? ??? ?? ??:?? "),
        }

        let mut full_path = PathBuf::new();
        for n in path.head {
            let n: &FsNode = n.fragment();
            full_path.push(&n.name);
            r.push_str(&n.display(&full_path, false));
        }

        let n: &FsNode = path.tail.fragment();
        full_path.push(&n.name);
        r.push_str(&n.display(&full_path, false));

        r
    }

    fn event_poller(&mut self) -> Option<EventPoller> {
        None // TODO
    }

    fn event_occured(&mut self, event: &Event) {
        let path = self.components(&event.path[..].into());
        match &event.kind {
            EventKind::Create(frag) => {
                todo!(
                    "fs: create {path:?} {:?}",
                    frag.as_any().downcast_ref::<FsNode>(),
                )
            }
            EventKind::Modify(dest, frag) => {
                todo!(
                    "fs: modify {path:?} {:?} {:?}",
                    dest.as_ref()
                        .map(|dest| self.components(&dest[..].into()))
                        .unwrap_or_default(),
                    frag.as_ref()
                        .map(|frag| frag.as_any().downcast_ref::<FsNode>()),
                )
            }
            EventKind::Copies(dest, frag) => {
                todo!(
                    "fs: copies {path:?} {:?} {:?}",
                    dest.as_ref()
                        .map(|dest| self.components(&dest[..].into()))
                        .unwrap_or_default(),
                    frag.as_ref()
                        .map(|frag| frag.as_any().downcast_ref::<FsNode>()),
                )
            }
            EventKind::Remove => todo!("fs: remove {path:?}"),
            EventKind::Reload => todo!("fs: reload {path:?}"),
        }
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
            Ok(Self(root))
        }
    }

    pub fn guess(arg: impl AsRef<Path>) -> bool {
        arg.as_ref().is_dir()
    }
}
