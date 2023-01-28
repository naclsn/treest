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

fn render_r(
    tree_node: &Node,
    state_node: &State,
    buf: &mut Buffer,
    (indent, line): (&mut u16, &mut u16),
    cursor_path: Option<&[usize]>,
) {
    let is_cursor = if let Some(v) = cursor_path {
        v.is_empty()
    } else {
        false
    };

    // this node
    {
        let name = {
            let file_name = tree_node.path.file_name().unwrap().to_str().unwrap();
            if is_cursor {
                format!("> {}", file_name)
            } else {
                file_name.to_string()
            }
        };
        buf.set_string(*indent * 3, *line, name, Style::default());
        *line += 1;
    }

    // recurse
    if state_node.unfolded {
        *indent += 1;
        let chs = tree_node.loaded_children().unwrap();
        for (in_state_idx, (tree_node, state_node)) in state_node
            .children
            .iter()
            .map(|(in_node_idx, stt)| (chs.get(*in_node_idx).unwrap(), stt))
            .enumerate()
        {
            render_r(
                tree_node,
                state_node,
                buf,
                (indent, line),
                cursor_path.and_then(|p_slice| {
                    if 0 == p_slice.len() {
                        return None;
                    }
                    let (head, tail) = p_slice.split_at(1);
                    if in_state_idx == head[0] {
                        return Some(tail);
                    }
                    None
                }),
            );
        }
        *indent -= 1;
    };
}

impl StatefulWidget for &mut Tree {
    type State = View;

    fn render(self, _area: Rect, buf: &mut Buffer, state: &mut View) {
        let mut line = 0;
        let mut indent = 0;

        render_r(
            &self.root,
            &state.root,
            buf,
            (&mut indent, &mut line),
            Some(&state.cursor),
        );

        buf.set_string(
            indent,
            line,
            format!("=== {:?}", state.cursor),
            Style::default(),
        );
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
