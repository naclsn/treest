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

const INDENT: &str = "\u{2502}\u{a0}\u{a0}"; // "|  "
const BRANCH: &str = "\u{251c}\u{2500}\u{2500}"; // "|--"
const BRANCH_LAST: &str = "\u{2514}\u{2500}\u{2500}"; // "L--"
const _TOP_OFFSCRN: &str = "\u{2506}\u{a0}\u{a0}"; // ...
const _BOT_OFFSCRN: &str = "\u{2506}\u{a0}\u{a0}"; // ...

const INDENT_W: u16 = 4;

fn render_name(
    tree_node: &Node,
    state_node: &State,
    buf: &mut Buffer,
    (indent, line): (u16, u16),
    area: Rect,
    is_cursor: bool,
) {
    if area.width <= indent {
        return;
    }

    let file_name = tree_node.file_name();
    let run_len = file_name.len();
    let avail_len = (area.width - indent) as usize;

    let sty = {
        let style = tree_node.style();
        if is_cursor {
            style.add_modifier(Modifier::REVERSED)
        } else {
            style
        }
    };

    let deco = tree_node.decoration();
    let deco_empty = if state_node.unfolded && state_node.children.is_empty() {
        " (/)"
    } else {
        ""
    };

    if run_len < avail_len {
        let c = Spans::from(vec![
            Span::styled(file_name, sty),
            Span::raw(deco),
            Span::raw(deco_empty),
        ]);

        buf.set_spans(area.x + indent, area.y + line, &c, area.width - indent);
    } else {
        let ext = tree_node.extension().unwrap_or("");
        let cut = 1 + ext.len() + deco.len() + deco_empty.len();

        let c = Spans::from(vec![
            Span::styled(&file_name[..avail_len - cut], sty),
            Span::styled("\u{2026}", sty),
            Span::styled(ext, sty),
            Span::raw(deco),
            Span::raw(deco_empty),
        ]);

        buf.set_spans(area.x + indent, area.y + line, &c, area.width - indent);
    }
}

fn render_r(
    tree_node: &Node,
    state_node: &State,
    buf: &mut Buffer,
    (indent, line): (&mut u16, &mut u16),
    area: Rect,
    cursor_path: Option<&[usize]>,
) {
    // if area.height <= *line + 1 {
    //     return;
    // }

    // this node
    render_name(
        tree_node,
        state_node,
        buf,
        (*indent, *line),
        area,
        if let Some(v) = cursor_path {
            v.is_empty()
        } else {
            false
        },
    );
    *line += 1;

    // recurse
    if state_node.unfolded && !state_node.children.is_empty() {
        *indent += INDENT_W;

        let count = state_node.children.len();
        let chs = tree_node.loaded_children().unwrap();

        for (in_state_idx, (tree_node, state_node)) in state_node
            .children
            .iter()
            .map(|(in_node_idx, stt)| (chs.get(*in_node_idx).unwrap(), stt))
            .enumerate()
        {
            if area.height <= *line {
                break;
            }

            let is_last = in_state_idx == count - 1;

            buf.set_string(
                area.x + *indent - INDENT_W,
                area.y + *line,
                if is_last { BRANCH_LAST } else { BRANCH },
                Style::default(),
            );

            let p_line = *line;
            render_r(
                tree_node,
                state_node,
                buf,
                (indent, line),
                area,
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
                        area.x + *indent - INDENT_W,
                        area.y + k,
                        INDENT,
                        Style::default(),
                    );
                }
            }
        }

        *indent -= INDENT_W;
    };
}

impl StatefulWidget for &mut Tree {
    type State = View;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut View) {
        let mut indent = 0;
        let mut line = 0;

        render_r(
            &self.root,
            &state.root,
            buf,
            (&mut indent, &mut line),
            area,
            Some(&state.cursor),
        );

        // buf.set_string(
        //     area.x + indent,
        //     area.y + line,
        //     format!("=== {:?}", state.cursor),
        //     Style::default(),
        // );
    }
}

impl Tree {
    pub fn new(path: PathBuf) -> io::Result<Tree> {
        Ok(Tree {
            root: Node::new_root(path),
        })
    }
}
