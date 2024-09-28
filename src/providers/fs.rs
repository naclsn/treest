use std::cmp::Ordering;
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use lscolors::{LsColors, Style};

use super::Error;
use crate::fisovec::FilterSorter;
use crate::tree::{Provider, ProviderExt};

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

#[inline(always)]
fn write_perm(f: &mut Formatter, perm: u32) -> FmtResult {
    write!(
        f,
        "{}{}{}",
        if perm >> 2 & 0b1 == 1 { 'r' } else { '-' },
        if perm >> 1 & 0b1 == 1 { 'w' } else { '-' },
        if perm & 0b1 == 1 { 'x' } else { '-' },
    )
}

#[cfg(unix)]
fn write_meta(f: &mut Formatter, node: &FsNode) -> FmtResult {
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
    write_perm(f, mode >> 6 & 0b111)?;
    // group
    write_perm(f, mode >> 3 & 0b111)?;
    // world
    write_perm(f, mode & 0b111)
}

#[cfg(windows)]
fn write_meta(f: &mut Formatter, node: &FsNode) -> FmtResult {
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

impl ProviderExt for Fs {
    fn fmt_frag_path(&self, f: &mut Formatter, path: Vec<&Self::Fragment>) -> FmtResult {
        let node = path.last().unwrap();
        write_meta(f, node)?;

        match &node.meta {
            Some(meta) => {
                write!(f, " {:8} ", meta.len())?;
                match meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                {
                    Some(duration) => {
                        let s = duration.as_secs();
                        write!(
                            f,
                            "{:02}:{:02}:{:02} ",
                            // TODO(+2): get tz properly, likely stealing from
                            // https://github.com/chronotope/chrono/tree/main/src/offset/local/tz_info
                            (s / 60 / 60) % 24 + 2,
                            (s / 60) % 60,
                            s % 60,
                        )
                    }
                    None => write!(f, "??:??:?? "),
                }
            }
            None => write!(f, "        ? ??:??:?? "),
        }?;

        path.iter().try_for_each(|it| write!(f, "{it}"))
    }
}

impl FilterSorter<FsNode> for Fs {
    fn compare(&self, a: &FsNode, b: &FsNode) -> Option<Ordering> {
        Some(Ord::cmp(&a.name, &b.name))
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
