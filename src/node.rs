use serde::{Deserialize, Serialize};
use std::{
    fmt::{self, Display, Formatter},
    fs::{read_dir, read_link, symlink_metadata, Metadata},
    io,
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize, Debug)]
enum FileKind {
    NamedPipe,
    CharDevice,
    BlockDevice,
    Regular,
    Socket,
    Executable,
}

#[derive(Serialize, Deserialize, Debug)]
enum NodeInfo {
    Dir { unfolded: bool, children: Vec<Node> },
    Link { target: Box<Node> },
    File { kind: FileKind },
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Node {
    path: PathBuf,
    marked: bool,
    info: NodeInfo,
}

impl Display for Node {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let depth = f.precision().unwrap_or(0);
        let ident = "   ".repeat(depth);

        let name = {
            let tmp = self.path.file_name().unwrap().to_str().unwrap();
            if self.marked {
                format!("[{}]", tmp)
            } else {
                tmp.to_string()
            }
        };

        match &self.info {
            NodeInfo::Dir { unfolded, children } => {
                write!(f, "{ident}{name}/",)?;
                if *unfolded {
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

            NodeInfo::Link { target } => write!(f, "{ident}{name}@ -> {}", target),

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
impl From<Metadata> for FileKind {
    fn from(meta: Metadata) -> FileKind {
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
impl From<Metadata> for FileKind {
    fn from(meta: Metadata) -> FileKind {
        FileKind::Regular
    }
}

impl Node {
    pub fn new(path: PathBuf, meta: Metadata) -> io::Result<Node> {
        let info = if meta.is_dir() {
            NodeInfo::Dir {
                unfolded: false,
                children: Vec::new(),
            }
        } else if meta.is_symlink() {
            NodeInfo::Link {
                target: Box::new({
                    let realpath = read_link(path.clone())?;
                    Node::new(realpath.clone(), symlink_metadata(realpath)?)?
                }),
            }
        } else {
            NodeInfo::File { kind: meta.into() }
        };
        Ok(Node {
            path,
            marked: false,
            info,
        })
    }

    pub fn new_root(path: PathBuf) -> Node {
        Node {
            path,
            marked: false,
            info: NodeInfo::Dir {
                unfolded: false,
                children: Vec::new(),
            },
        }
    }

    pub fn as_path(&self) -> &Path {
        self.path.as_path()
    }

    pub fn mark(&mut self, ed: bool) {
        self.marked = ed;
    }
    // pub fn marked(&self) -> bool {
    //     self.marked
    // }

    pub fn unfold(&mut self) -> io::Result<&mut Vec<Node>> {
        match &mut self.info {
            NodeInfo::Dir { unfolded, children } => {
                if !*unfolded {
                    *children = read_dir(self.path.clone())?
                        .map(|maybe_ent| {
                            maybe_ent.and_then(|ent| {
                                ent.metadata().and_then(|meta| Node::new(ent.path(), meta))
                            })
                        })
                        .collect::<Result<Vec<Node>, _>>()?;
                    *unfolded = true;
                }
                Ok(children)
            }

            NodeInfo::Link { target } => target.unfold(),

            NodeInfo::File { .. } => Err(io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "(NotADirectory) cannot unfold file at {}",
                    self.path.display()
                ),
            )),
        }
    }
}
