use crate::{node::Node, view::View};
use tui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    widgets::StatefulWidget,
};
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

impl StatefulWidget for &mut Tree {
    type State = View;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        buf.set_string(2, 2, "coucoucoucoucoucoucoucoucoucoucoucoucoucoucoucoucoucoucoucoucoucoucoucou", Style::default());
        self.at("".into());
    }
}

impl Tree {
    pub fn new(path: PathBuf) -> io::Result<Tree> {
        Ok(Tree { root: Node::new_root(path) })
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
                    .children()?
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
}
