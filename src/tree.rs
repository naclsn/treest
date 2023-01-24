use std::fmt;
use std::fs;
use std::io;
use std::path;

#[derive(Debug)]
enum FileKind {
    NamedPipe,
    CharDevice,
    BlockDevice,
    Regular,
    Socket,
    Executable,
}

#[derive(Debug)]
enum NodeInfo {
    Dir { unfolded: bool, children: Vec<Node> },
    Link { target: Box<Node> },
    File { kind: FileKind },
}

#[derive(Debug)]
pub struct Node {
    path: path::PathBuf,
    info: NodeInfo,
}

#[derive(Debug)]
pub struct Tree {
    root: Node,
}

impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let depth = f.precision().unwrap_or(0);
        let ident = "   ".repeat(depth);
        let name = self.path.file_name().unwrap().to_str().unwrap();
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

            NodeInfo::Link { target } => writeln!(f, "{ident}{name}@ -> {}", target),

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
impl From<fs::Metadata> for FileKind {
    fn from(meta: fs::Metadata) -> FileKind {
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
impl From<fs::Metadata> for FileKind {
    fn from(meta: fs::Metadata) -> FileKind {
        FileKind::Regular
    }
}

impl Node {
    fn new(path: path::PathBuf, meta: fs::Metadata) -> io::Result<Node> {
        Ok(Node {
            // YYY: fields not in order so the `path.clone()` only
            // occurs when needed (in the `is_link` branch)
            info: if meta.is_dir() {
                NodeInfo::Dir {
                    unfolded: false,
                    children: Vec::new(),
                }
            } else if meta.is_symlink() {
                NodeInfo::Link {
                    target: Box::new({
                        let realpath = fs::read_link(path.clone())?;
                        Node::new(realpath.clone(), fs::symlink_metadata(realpath)?)?
                    }),
                }
            } else {
                NodeInfo::File { kind: meta.into() }
            },
            path,
        })
    }

    fn unfold(&mut self) -> io::Result<&mut Vec<Node>> {
        match &mut self.info {
            NodeInfo::Dir { unfolded, children } => {
                if !*unfolded {
                    *children = fs::read_dir(self.path.clone())?
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

impl fmt::Display for Tree {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.root)
    }
}

impl Tree {
    pub fn new(path: path::PathBuf) -> io::Result<Tree> {
        Ok(Tree {
            root: {
                let mut root = Node {
                    path,
                    info: NodeInfo::Dir {
                        unfolded: false,
                        children: Vec::new(),
                    },
                };
                root.unfold()?;
                root
            },
        })
    }

    pub fn unfold_at(&mut self, path: path::PathBuf) -> io::Result<&mut Vec<Node>> {
        let mut cursor = &mut self.root;
        for co in path.components() {
            cursor = match co {
                path::Component::Prefix(_) | path::Component::RootDir => Err(io::Error::new(
                    io::ErrorKind::Other,
                    "not supported: absolute paths",
                )),

                path::Component::CurDir => Ok(cursor),

                path::Component::ParentDir => panic!("parent dir (gave up)"),

                path::Component::Normal(path_comp) => cursor
                    .unfold()?
                    .iter_mut()
                    .find(|ch| match ch.path.file_name() {
                        Some(ch_head) => path_comp == ch_head,
                        _ => false,
                    })
                    .ok_or(io::Error::from(io::ErrorKind::NotFound)),
            }?
        }
        cursor.unfold()
    }
}
