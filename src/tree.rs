use crate::{
    node::Node,
    view::{State, View},
};
use serde::{Deserialize, Serialize};
use std::{
    fmt::{self, Display, Formatter},
    io,
    path::PathBuf,
};
use tui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Span, Spans},
    widgets::StatefulWidget,
};

#[derive(Serialize, Deserialize, Debug)]
pub struct Tree {
    pub root: Node,
}

impl Display for Tree {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}", self.root)
    }
}

const INDENT: &str = "\u{2502}\u{a0}\u{a0}"; // _VE _SP _SP " ";
const _INDENT_LAST: &str = "\u{a0}\u{a0}\u{a0}"; // _SP _SP _SP " ";
const BRANCH: &str = "\u{251c}\u{2500}\u{2500}"; // _TE _HZ _HZ " ";
const BRANCH_LAST: &str = "\u{2514}\u{2500}\u{2500}"; // _AN _HZ _HZ " ";
const _TOP_OFFSCRN: &str = "\u{2506}\u{a0}\u{a0}"; // _UP _SP _SP " ";
const _BOT_OFFSCRN: &str = "\u{2506}\u{a0}\u{a0}"; // _DW _SP _SP " ";

fn render_r(
    tree_node: &Node,
    state_node: &State,
    buf: &mut Buffer,
    (indent, line): (&mut u16, &mut u16),
    cursor_path: Option<&[usize]>,
) {
    // this node
    {
        let is_cursor = if let Some(v) = cursor_path {
            v.is_empty()
        } else {
            false
        };

        let c = Spans::from(vec![
            Span::styled(tree_node.file_name(), {
                let style = tree_node.style();
                if is_cursor {
                    style.add_modifier(Modifier::REVERSED)
                } else {
                    style
                }
            }),
            Span::raw(tree_node.decoration()),
        ]);

        buf.set_spans(*indent * 4, *line, &c, 14);
        *line += 1;
    }

    // recurse
    if state_node.unfolded && !state_node.children.is_empty() {
        *indent += 1;

        let count = state_node.children.len();
        let chs = tree_node.loaded_children().unwrap();

        for (in_state_idx, (tree_node, state_node)) in state_node
            .children
            .iter()
            .map(|(in_node_idx, stt)| (chs.get(*in_node_idx).unwrap(), stt))
            .enumerate()
        {
            let is_last = in_state_idx == count - 1;

            buf.set_string(
                (*indent - 1) * 4,
                *line,
                if is_last { BRANCH_LAST } else { BRANCH },
                Style::default(),
            );

            let p_line = *line;
            render_r(
                tree_node,
                state_node,
                buf,
                (indent, line),
                cursor_path.and_then(|p_slice| {
                    if p_slice.is_empty() {
                        return None;
                    }
                    let (head, tail) = p_slice.split_at(1);
                    if in_state_idx == head[0] {
                        return Some(tail);
                    }
                    None
                }),
            );

            if !is_last {
                for k in p_line + 1..*line {
                    buf.set_string(
                        (*indent - 1) * 4,
                        k,
                        INDENT, //if is_last { INDENT_LAST } else { INDENT },
                        Style::default(),
                    );
                }
            }
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

    // pub fn at(&mut self, path: PathBuf) -> io::Result<&mut Node> {
    //     let mut cursor = &mut self.root;
    //     for co in path.components() {
    //         cursor = match co {
    //             Component::Prefix(_) | Component::RootDir => Err(io::Error::new(
    //                 io::ErrorKind::Other,
    //                 "not supported: absolute paths",
    //             )),
    //
    //             Component::CurDir => Ok(cursor),
    //
    //             Component::ParentDir => todo!("parent dir"),
    //
    //             Component::Normal(path_comp) => cursor
    //                 .load_children()?
    //                 .iter_mut()
    //                 .find(|ch| match ch.as_path().file_name() {
    //                     Some(ch_head) => path_comp == ch_head,
    //                     _ => false,
    //                 })
    //                 .ok_or(io::Error::from(io::ErrorKind::NotFound)),
    //         }?
    //     }
    //     Ok(cursor)
    // }
}
