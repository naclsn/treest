use std::env;
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
struct Node {
    path: path::PathBuf,
    info: NodeInfo,
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
                NodeInfo::File {
                    kind: FileKind::Regular,
                }
            },
            path,
        })
    }

    fn unfold(&mut self) -> io::Result<()> {
        match &mut self.info {
            NodeInfo::Dir { unfolded, children } => {
                *children = fs::read_dir(self.path.clone())?
                    .map(|maybe_ent| {
                        maybe_ent.and_then(|ent| {
                            ent.metadata().and_then(|meta| Node::new(ent.path(), meta))
                        })
                    })
                    .collect::<Result<Vec<Node>, _>>()?;
                *unfolded = true;
                Ok(())
            }

            NodeInfo::Link { target } => target.unfold(),

            NodeInfo::File { .. } => Err(io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "(NotADirectory) cannot unfold file as {}",
                    self.path.display()
                ),
            )),
        }
    }
}

fn main() {
    let mut root = Node {
        path: env::current_dir().unwrap(),
        info: NodeInfo::Dir {
            unfolded: false,
            children: Vec::new(),
        },
    };
    root.unfold().expect("could not unfold root");

    print!("{root}");
    println!("---");

    match &mut root.info {
        NodeInfo::Dir {
            unfolded: _,
            children,
        } => {
            for ch in children {
                ch.unfold().ok();
            }
        }
        _ => unreachable!(),
    }

    print!("{root}");
    println!("---");
}
