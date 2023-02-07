use serde::{Deserialize, Serialize};
use std::{
    fmt::{self, Display, Formatter},
    fs::{metadata, read_dir, read_link, symlink_metadata, Metadata},
    io,
    path::{Path, PathBuf},
};
use tui::style::{Color, Modifier, Style};

#[derive(Serialize, Deserialize, Debug)]
pub enum FileKind {
    NamedPipe,
    CharDevice,
    BlockDevice,
    Regular,
    Socket,
    Executable,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum NodeInfo {
    Dir { loaded: bool, children: Vec<Node> },
    Link { target: Result<Box<Node>, PathBuf> },
    File { kind: FileKind },
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Node {
    path: PathBuf,
    #[serde(skip_serializing, skip_deserializing)]
    meta: Option<Metadata>,
    info: NodeInfo,
}

impl Display for Node {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let depth = f.precision().unwrap_or(0);
        let ident = "   ".repeat(depth);

        let name = self.path.file_name().unwrap().to_str().unwrap();

        match &self.info {
            NodeInfo::Dir { loaded, children } => {
                write!(f, "{ident}{name}/",)?;
                if *loaded {
                    if children.is_empty() {
                        writeln!(f, " (/)")
                    } else {
                        writeln!(f)?;
                        children
                            .iter()
                            .map(|ch| write!(f, "{:.*}", depth + 1, ch))
                            .collect()
                    }
                } else {
                    writeln!(f)
                }
            }

            NodeInfo::Link { target } => {
                write!(f, "{ident}{name}@ -> ")?;
                match target {
                    Ok(node) => write!(f, "{node}"),
                    Err(path) => write!(f, "~{}~", path.to_string_lossy()),
                }
            }

            NodeInfo::File { kind } => match kind {
                FileKind::NamedPipe => writeln!(f, "{ident}{name}|"),
                FileKind::Socket => writeln!(f, "{ident}{name}="),
                FileKind::Executable => writeln!(f, "{ident}{name}*"),
                _ => writeln!(f, "{ident}{name}"),
            },
        }
    }
}

#[cfg(unix)]
impl From<&Metadata> for FileKind {
    fn from(meta: &Metadata) -> FileKind {
        use std::os::unix::fs::FileTypeExt;
        use std::os::unix::fs::PermissionsExt;
        let ft = meta.file_type();
        if ft.is_fifo() {
            FileKind::NamedPipe
        } else if ft.is_char_device() {
            FileKind::CharDevice
        } else if ft.is_block_device() {
            FileKind::BlockDevice
        } else if ft.is_socket() {
            FileKind::Socket
        } else if meta.permissions().mode() & 0o111 != 0 {
            FileKind::Executable
        } else {
            FileKind::Regular
        }
    }
}

#[cfg(windows)]
impl From<&Metadata> for FileKind {
    fn from(meta: &Metadata) -> FileKind {
        FileKind::Regular
    }
}

fn perm_to_string(o: u32) -> String {
    [
        if o >> 2 & 0b1 == 1 { 'r' } else { '-' },
        if o >> 1 & 0b1 == 1 { 'w' } else { '-' },
        if o >> 0 & 0b1 == 1 { 'x' } else { '-' },
    ]
    .into_iter()
    .collect()
}

#[cfg(unix)]
fn meta_to_string(meta: &Metadata) -> String {
    use std::os::unix::fs::FileTypeExt;
    use std::os::unix::fs::PermissionsExt;

    let mode = meta.permissions().mode();
    let ft = meta.file_type();

    [
        // file type
        if ft.is_block_device() {
            'b'
        } else if ft.is_char_device() {
            'c'
        } else if ft.is_dir() {
            'd'
        } else if ft.is_symlink() {
            'l'
        } else if ft.is_fifo() {
            'p'
        } else if ft.is_socket() {
            's'
        } else {
            '-'
        }
        .to_string(),
        // owner
        perm_to_string(mode >> 3 * 2 & 0b111),
        // group
        perm_to_string(mode >> 3 * 1 & 0b111),
        // world
        perm_to_string(mode >> 3 * 0 & 0b111),
    ]
    .concat()
}

#[cfg(windows)]
fn meta_to_string(meta: &Metadata) -> String {
    use std::os::windows::fs::FileTypeExt;
    use std::os::windows::fs::PermissionsExt;

    let ro = meta.permissions().readonly();
    let ft = meta.file_type();

    [
        // file type
        if ft.is_dir() {
            'd'
        } else if ft.is_symlink() {
            'l'
        } else {
            '-'
        }
        .to_string(),
        // owner
        perm_to_string(0b101 | if ro { 0b000 } else { 0b010 }),
        // group
        perm_to_string(0b101 | if ro { 0b000 } else { 0b010 }),
        // world
        perm_to_string(0b101 | if ro { 0b000 } else { 0b010 }),
    ]
    .concat()
}

impl Node {
    pub fn new(path: PathBuf, meta: Metadata) -> io::Result<Node> {
        let info = if meta.is_dir() {
            NodeInfo::Dir {
                loaded: false,
                children: Vec::new(),
            }
        } else if meta.is_symlink() {
            NodeInfo::Link {
                target: {
                    let realpath = read_link(path.clone())?;
                    symlink_metadata(realpath.clone())
                        .and_then(|lnmeta| Node::new(realpath.clone(), lnmeta))
                        .map(|node| Box::new(node))
                        .map_err(|_| realpath)
                },
            }
        } else {
            NodeInfo::File {
                kind: (&meta).into(),
            }
        };

        Ok(Node {
            path,
            meta: Some(meta),
            info,
        })
    }

    pub fn new_root(path: PathBuf) -> io::Result<Node> {
        let meta = Some(metadata(&path)?);
        Ok(Node {
            path,
            meta,
            info: NodeInfo::Dir {
                loaded: false,
                children: Vec::new(),
            },
        })
    }

    pub fn as_path(&self) -> &Path {
        self.path.as_path()
    }

    pub fn loaded_children(&self) -> Option<&Vec<Node>> {
        match &self.info {
            NodeInfo::Dir { loaded, children } if *loaded => Some(children),

            NodeInfo::Link { target: Ok(target) } => target.loaded_children(),

            _ => None,
        }
    }

    pub fn loaded_children_mut(&mut self) -> Option<&mut Vec<Node>> {
        match &mut self.info {
            NodeInfo::Dir { loaded, children } if *loaded => Some(children),

            NodeInfo::Link { target: Ok(target) } => target.loaded_children_mut(),

            _ => None,
        }
    }

    pub fn load_children(&mut self) -> io::Result<&mut Vec<Node>> {
        match &mut self.info {
            NodeInfo::Dir { loaded, children } => {
                if !*loaded {
                    *children = read_dir(self.path.clone())?
                        .map(|maybe_ent| {
                            maybe_ent.and_then(|ent| {
                                ent.metadata().and_then(|meta| Node::new(ent.path(), meta))
                            })
                        })
                        .collect::<Result<_, _>>()?;
                    *loaded = true;
                }
                Ok(children)
            }

            NodeInfo::Link { target } => match target {
                Ok(node) => node.load_children(),
                Err(path) => Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!(
                        "(NotADirectory) cannot unfold file at {}",
                        path.to_string_lossy()
                    ),
                )),
            },

            NodeInfo::File { .. } => Err(io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "(NotADirectory) cannot unfold file at {}",
                    self.path.to_string_lossy()
                ),
            )),
        }
    }

    pub fn meta_to_string(&self) -> String {
        match &self.meta {
            Some(meta) => meta_to_string(meta),
            None => "- no meta - ".to_string(),
        }
    }

    pub fn fixup_meta(&mut self) -> io::Result<()> {
        self.meta = Some(self.path.metadata()?);
        Ok(())
    }

    pub fn fixup(&mut self) -> bool {
        let Ok(()) = self.fixup_meta() else { return false };
        match &mut self.info {
            NodeInfo::Dir {
                loaded: true,
                children,
            } => {
                // FIXME: raw edits to the vec will cause a panic at unwrap in some view!
                children.retain_mut(Node::fixup);
            }
            NodeInfo::Link { target } => {
                if let Ok(node) = target {
                    node.fixup();
                }
            }
            _ => (),
        }
        true
    }

    pub fn file_name(&self) -> &str {
        self.path.file_name().unwrap().to_str().unwrap()
    }

    pub fn extension(&self) -> Option<&str> {
        self.path.extension().and_then(|ostr| ostr.to_str())
    }

    pub fn decoration(&self) -> String {
        match &self.info {
            NodeInfo::Dir { .. } => "/".to_string(),
            NodeInfo::Link { target } => match target {
                Ok(node) => format!("@ -> {}{}", node.file_name(), node.decoration()),
                Err(path) => format!("@ ~> {}", path.to_string_lossy()),
            },
            NodeInfo::File { kind } => match kind {
                FileKind::NamedPipe => "|",
                FileKind::Socket => "=",
                FileKind::Executable => "*",
                _ => "",
            }
            .to_string(),
        }
    }

    pub fn style(&self) -> Style {
        let r = Style::default();
        match &self.info {
            NodeInfo::Dir { .. } => r.fg(Color::Blue).add_modifier(Modifier::BOLD),
            NodeInfo::Link { target } => match target {
                Ok(_node) => r.fg(Color::Cyan).add_modifier(Modifier::BOLD), // LS_COLOR allows using target's
                Err(_path) => r
                    .fg(Color::Red)
                    .bg(Color::Black)
                    .add_modifier(Modifier::CROSSED_OUT),
            },
            NodeInfo::File { kind } => match kind {
                FileKind::NamedPipe => r.fg(Color::Yellow).bg(Color::Black),
                FileKind::CharDevice => r
                    .fg(Color::Yellow)
                    .bg(Color::Black)
                    .add_modifier(Modifier::BOLD),
                FileKind::BlockDevice => r
                    .fg(Color::Yellow)
                    .bg(Color::Black)
                    .add_modifier(Modifier::BOLD),
                FileKind::Regular => r,
                FileKind::Socket => r.fg(Color::Magenta).add_modifier(Modifier::BOLD),
                FileKind::Executable => r.fg(Color::Green).add_modifier(Modifier::BOLD),
            },
        }
    }
}
