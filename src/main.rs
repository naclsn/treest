use std::path::PathBuf;

#[derive(Debug)]
enum FileKind {
    NamedPipe,
    CharDevice,
    BlockDevice,
    Regular,
    Sucket,
    Executable,
}

#[derive(Debug)]
struct File {
    path: PathBuf,
    kind: FileKind,
}

#[derive(Debug)]
struct Dir {
    path: PathBuf,
    unfolded: bool,
    children: Vec<Node>,
}

#[derive(Debug)]
struct Link {
    path: PathBuf,
    target: Box<Node>,
}

#[derive(Debug)]
enum Node {
    Dir(Dir),
    Link(Link),
    File(File),
}

fn open_dir(path: PathBuf) -> Dir {
    Dir {
        path,
        unfolded: false,
        children: Vec::new(),
    }
}

fn main() {
    // let _root = open_dir("".to_string());
    let idk = PathBuf::new();
    idk.extension();
    println!("coucou");
}
