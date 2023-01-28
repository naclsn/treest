use crate::{
    node::Node,
    view::{State, View},
};
use serde::{Deserialize, Serialize};
use std::{
    fmt::{self, Display, Formatter},
    io,
    path::{Component, PathBuf},
};
use tui::{buffer::Buffer, layout::Rect, style::Style, widgets::StatefulWidget};

#[derive(Serialize, Deserialize, Debug)]
pub struct Tree {
    pub root: Node,
}

impl Display for Tree {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}", self.root)
    }
}

impl StatefulWidget for &mut Tree {
    type State = View;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let mut line = 0;
        let mut indent = 0u16;

        let tree_node = &mut self.root;
        let state_node = &state.root;

        // this node
        {
            let name = tree_node.path.file_name().unwrap().to_str().unwrap();
            buf.set_string(indent, line, name, Style::default());
            line += 1;
        }

        // recurse
        if state_node.unfolded {
            indent += 1;
            let chs = tree_node.load_children().unwrap();
            for (tree_node, state_node) in state
                .root
                .children
                .iter()
                .map(|(idx, stt)| (chs.get(*idx).unwrap(), stt))
            {
                // this node
                {
                    let name = tree_node.path.file_name().unwrap().to_str().unwrap();
                    buf.set_string(indent, line, name, Style::default());
                    line += 1;
                }

                // recurse
                if state_node.unfolded {
                    indent += 1;
                    // ...
                    buf.set_string(indent, line, "---", Style::default());
                    line += 1;
                    indent -= 1;
                }
            }
            indent -= 1;
        };

        buf.set_string(indent, line, "===", Style::default());
    }
}

impl Tree {
    pub fn new(path: PathBuf) -> io::Result<Tree> {
        Ok(Tree {
            root: Node::new_root(path),
        })
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
                    .load_children()?
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
