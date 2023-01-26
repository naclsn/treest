use crate::node::Node;
use serde::{Deserialize, Serialize};
use std::{
    fmt::{self, Display, Formatter},
    io,
    path::{Component, PathBuf},
};

#[derive(Serialize, Deserialize, Debug)]
pub struct Tree {
    root: Node,
}

impl Display for Tree {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}", self.root)
    }
}

impl Tree {
    pub fn new(path: PathBuf) -> io::Result<Tree> {
        let mut root = Node::new_root(path);
        root.unfold()?;

        Ok(Tree { root })
    }

    pub fn at(&mut self, path: PathBuf) -> io::Result<&mut Node> {
        let mut cursor = &mut self.root;
        for co in path.components() {
            cursor = match co {
                Component::Prefix(_) | Component::RootDir => Err(io::Error::new(
                    io::ErrorKind::Other,
                    "not supported: absolute paths",
                )),

                Component::CurDir => Ok(cursor),

                Component::ParentDir => todo!("parent dir"),

                Component::Normal(path_comp) => cursor
                    .unfold()?
                    .iter_mut()
                    .find(|ch| match ch.as_path().file_name() {
                        Some(ch_head) => path_comp == ch_head,
                        _ => false,
                    })
                    .ok_or(io::Error::from(io::ErrorKind::NotFound)),
            }?
        }
        Ok(cursor)
    }

    pub fn unfold_at(&mut self, path: PathBuf) -> io::Result<&mut Vec<Node>> {
        self.at(path)?.unfold()
    }
}
