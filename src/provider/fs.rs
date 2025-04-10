use std::cmp::Ordering;
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::fs::{self, File, Metadata};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::Result;
use lscolors::{LsColors, Style};
use thiserror::Error;

use crate::provider::{Event, Provider};
use crate::tree::{Fragment, NodePath};

pub struct Fs(PathBuf);

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
    meta: Option<Metadata>,
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
        let Some(meta) = value.1 else { return Regular };

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
            Directory => 'd',
            SymLink(_) => 'l',
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
            SymLink(_) => 'l',
            _ => '-',
        },
        write_perm(0b101 | if ro { 0b000 } else { 0b010 }), // owner
        write_perm(0b101 | if ro { 0b000 } else { 0b010 }), // group
        write_perm(0b101 | if ro { 0b000 } else { 0b010 }), // world
    )
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
                .style_for_path_with_metadata(&self.name, self.meta.as_ref())
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
    fn provide_root(&self) -> Fragment {
        Box::new(FsNode {
            kind: Directory,
            name: self.0.to_string_lossy().trim_end_matches('/').to_string(),
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

        let mut r = Vec::<Fragment>::new();
        for entry in dir.filter_map(Result::ok) {
            let Some(name) = entry.file_name().into_string().ok() else {
                continue;
            };
            let meta = entry.metadata().ok();
            r.push(Box::new(FsNode {
                kind: (entry.path(), &meta).into(),
                name,
                meta,
            }))
        }
        r
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
        node.to_string()
    }

    fn components(&self, path: &NodePath) -> Vec<String> {
        path.head
            .iter()
            .chain(std::iter::once(&path.tail))
            .map(|n| n.fragment::<FsNode>().name.clone())
            .collect()
    }

    fn join(&self, components: &[String]) -> String {
        components.join("/")
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
                {
                    Some(duration) => {
                        let s = duration.as_secs();
                        r.push_str(&format!(
                            "{:02}:{:02}:{:02} ",
                            // TODO(+2): get tz properly, likely stealing from
                            // https://github.com/chronotope/chrono/tree/main/src/offset/local/tz_info
                            (s / 60 / 60) % 24 + 2,
                            (s / 60) % 60,
                            s % 60,
                        ));
                    }
                    None => r.push_str("??:??:?? "),
                }
            }
            None => r.push_str("        ? ??:??:?? "),
        }

        for n in path.head {
            r.push_str(&n.fragment::<FsNode>().to_string());
        }
        r.push_str(&path.tail.fragment::<FsNode>().to_string());

        r
    }

    fn request_mk(&mut self, path: &NodePath, text: String) -> Result<Option<Event>> {
        writeln!(
            File::options()
                .append(true)
                .open("./would.notquite.sh")
                .unwrap(),
            "mk {:?} {text:?}",
            self.components(path).join("/"),
        )?;
        Ok(None)
    }

    fn request_cp(&mut self, path: &NodePath, text: String) -> Result<Option<Event>> {
        writeln!(
            File::options()
                .append(true)
                .open("./would.notquite.sh")
                .unwrap(),
            "cp {:?} {text:?}",
            self.components(path).join("/"),
        )?;
        Ok(None)
    }

    fn request_rm(&mut self, path: &NodePath) -> Result<Option<Event>> {
        writeln!(
            File::options()
                .append(true)
                .open("./would.notquite.sh")
                .unwrap(),
            "rm {:?}",
            self.components(path).join("/"),
        )?;
        Ok(None)
    }

    fn request_mv(&mut self, path: &NodePath, text: String) -> Result<Option<Event>> {
        self.request_cp(path, text)?;
        self.request_rm(path)?;
        Ok(None)
    }

    fn request_ch(&mut self, path: &NodePath, text: String) -> Result<Option<Event>> {
        writeln!(
            File::options()
                .append(true)
                .open("./would.notquite.sh")
                .unwrap(),
            "ch {:?} {text:?}",
            self.components(path).join("/"),
        )?;
        Ok(None)
    }

    fn request_vi(&mut self, path: &NodePath) -> Result<Vec<String>> {
        let bytes = fs::read(self.components(path).into_iter().collect::<PathBuf>())?;
        Ok(String::from_utf8_lossy(&bytes)
            .lines()
            .map(String::from)
            .collect())
    }

    fn request_ex(
        &mut self,
        path: &NodePath,
        text: String,
    ) -> Result<(Vec<String>, Option<Event>)> {
        _ = (path, text);
        Ok((Vec::new(), None))
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
}
