use crate::node;
use serde;
use std::fmt;
use std::io;
use std::path;

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct Tree {
    root: node::Node,
    // cursor: &Node,
    // selection: Vec<&Node>,
}

impl fmt::Display for Tree {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.root)
    }
}

impl Tree {
    pub fn new(path: path::PathBuf) -> io::Result<Tree> {
        let mut root = node::Node::new_root(path);
        root.unfold()?;
        Ok(Tree {
            root,
            // cursor: root,
            // selection: vec![],
        })
    }

    pub fn at(&mut self, path: path::PathBuf) -> io::Result<&mut node::Node> {
        let mut cursor = &mut self.root;
        for co in path.components() {
            cursor = match co {
                path::Component::Prefix(_) | path::Component::RootDir => Err(io::Error::new(
                    io::ErrorKind::Other,
                    "not supported: absolute paths",
                )),

                path::Component::CurDir => Ok(cursor),

                path::Component::ParentDir => todo!("parent dir"),

                path::Component::Normal(path_comp) => cursor
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

    pub fn unfold_at(&mut self, path: path::PathBuf) -> io::Result<&mut Vec<node::Node>> {
        self.at(path)?.unfold()
    }
}
