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
struct File {
    kind: FileKind,
}

#[derive(Debug)]
struct Dir {
    unfolded: bool,
    children: Vec<Node>,
}

#[derive(Debug)]
struct Link {
    target: Box<Node>,
}

#[derive(Debug)]
enum NodeInfo {
    Dir(Dir),
    Link(Link),
    File(File),
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
            NodeInfo::Dir(dir) => {
                write!(f, "{ident}{name}/",)?;
                if dir.unfolded {
                    if dir.children.is_empty() {
                        writeln!(f, " (/)")
                    } else {
                        writeln!(f)?;
                        dir.children
                            .iter()
                            .map(|ch| write!(f, "{:.*}", depth + 1, ch))
                            .collect()
                    }
                } else {
                    writeln!(f)
                }
            }
            NodeInfo::Link(link) => writeln!(f, "{ident}{name}@ -> {}", link.target),
            NodeInfo::File(file) => match file.kind {
                FileKind::NamedPipe => writeln!(f, "{ident}{name}|"),
                FileKind::Socket => writeln!(f, "{ident}{name}="),
                FileKind::Executable => writeln!(f, "{ident}{name}*"),
                _ => writeln!(f, "{ident}{name}"),
            },
        }
    }
}

impl Node {
    fn unfold(&mut self) -> io::Result<()> {
        match &mut self.info {
            NodeInfo::Dir(dir) => {
                dir.children = fs::read_dir(self.path.clone())?
                    .map(|maybe_it| {
                        maybe_it
                            .and_then(|it| it.file_type().map(|ft| (it.path(), ft)))
                            .map(|(path, ft)| {
                                Node {
                                    path,
                                    info: if ft.is_dir() {
                                        NodeInfo::Dir(Dir {
                                            unfolded: false,
                                            children: Vec::new(),
                                        })
                                    } else if ft.is_symlink() {
                                        NodeInfo::Link(Link {
                                            // ZZZ: todo
                                            target: Box::new(Node {
                                                path: path::PathBuf::new(),
                                                info: NodeInfo::File(File {
                                                    kind: FileKind::Regular,
                                                }),
                                            }),
                                        })
                                    } else {
                                        NodeInfo::File(File {
                                            kind: FileKind::Regular,
                                        })
                                    },
                                }
                            })
                    })
                    .collect::<Result<Vec<Node>, _>>()?;
                dir.unfolded = true;
                Ok(())
            }
            NodeInfo::Link(link) => link.target.unfold(),
            NodeInfo::File(file) => Err(io::Error::new(
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
        info: NodeInfo::Dir(Dir {
            unfolded: false,
            children: Vec::new(),
        }),
    };
    root.unfold().expect("could not unfold root");

    print!("{root}");
    println!("---");

    match &mut root.info {
        NodeInfo::Dir(dir) => {
            for ch in &mut dir.children {
                ch.unfold().ok();
            }
        }
        _ => unreachable!(),
    }

    print!("{root}");
    println!("---");
}
