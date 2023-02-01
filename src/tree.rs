use crate::{
    node::Node,
    view::{Offset, State, View},
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
    style::{Color, Modifier, Style},
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

const INDENT: &str = "\u{2502}\u{a0}\u{a0}"; //          "|  "
const BRANCH: &str = "\u{251c}\u{2500}\u{2500}"; //      "|--"
const BRANCH_LAST: &str = "\u{2514}\u{2500}\u{2500}"; // "`--"
const _TOP_OFFSCRN: &str = "\u{2506}\u{a0}\u{a0}"; //     ...
const _BOT_OFFSCRN: &str = "\u{2506}\u{a0}\u{a0}"; //     ...
const INDENT_WIDTH: u16 = 4;

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
    let sty = {
        let style = if state_node.marked {
            Style::default()
                .fg(Color::Yellow)
                .bg(Color::Black)
                .add_modifier(Modifier::BOLD)
        } else {
            tree_node.style()
        };
        if is_cursor {
            style.add_modifier(Modifier::REVERSED)
        } else {
            style
        }
    };

    let deco = tree_node.decoration();

    let raw_prefix = Span::styled(if state_node.marked { " " } else { "" }, sty);
    let raw_suffix = Span::raw(if state_node.unfolded && state_node.children.is_empty() {
        " (/)"
    } else {
        ""
    });

    let run_len = file_name.len();
    let avail_len = (area.width - indent) as usize;
    if run_len < avail_len {
        let c = Spans::from(vec![
            raw_prefix,
            Span::styled(file_name, sty),
            Span::raw(deco),
            raw_suffix,
        ]);

        buf.set_spans(area.x + indent, area.y + line, &c, area.width - indent);
    } else {
        let ext = tree_node.extension().unwrap_or("");
        let cut = 1 + ext.len() + deco.len() + raw_prefix.width() + raw_suffix.width();

        let c = Spans::from(vec![
            raw_prefix,
            Span::styled(&file_name[..avail_len - cut], sty),
            Span::styled("\u{2026}", sty),
            Span::styled(ext, sty),
            Span::raw(deco),
            raw_suffix,
        ]);

        buf.set_spans(area.x + indent, area.y + line, &c, area.width - indent);
    }
}

fn render_r(
    tree_node: &Node,
    state_node: &State,
    buf: &mut Buffer,
    curr: &mut Offset,
    area: Rect,
    cursor_path: Option<&[usize]>,
) {
    // XXX: too many numeric type conversion, either
    // that's normal rust, or there's a bigger problem

    // this node
    if 0 <= curr.shift && 0 <= curr.scroll {
        render_name(
            tree_node,
            state_node,
            buf,
            (curr.shift as u16, curr.scroll as u16),
            area,
            if let Some(v) = cursor_path {
                v.is_empty()
            } else {
                false
            },
        );
    }
    curr.scroll += 1;

    // recurse
    if state_node.unfolded && !state_node.children.is_empty() {
        curr.shift += INDENT_WIDTH as i32;

        let count = state_node.children.len();
        let chs = tree_node.loaded_children().unwrap();

        for (in_state_idx, (tree_node, state_node)) in state_node
            .children
            .iter()
            .map(|(in_node_idx, stt)| (chs.get(*in_node_idx).unwrap(), stt))
            .enumerate()
        {
            if area.height as i32 <= curr.scroll {
                break;
            }

            let is_last = in_state_idx == count - 1;

            if INDENT_WIDTH as i32 <= curr.shift && 0 <= curr.scroll {
                buf.set_string(
                    area.x + (curr.shift as u16) - INDENT_WIDTH,
                    area.y + (curr.scroll as u16),
                    if is_last { BRANCH_LAST } else { BRANCH },
                    Style::default(),
                );
            }

            let p_line = curr.scroll;
            render_r(
                tree_node,
                state_node,
                buf,
                curr,
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
                let start = if p_line + 1 < 0 { 0 } else { p_line + 1 };
                for k in start..curr.scroll {
                    if INDENT_WIDTH as i32 <= curr.shift {
                        buf.set_string(
                            area.x + (curr.shift as u16) - INDENT_WIDTH,
                            area.y + (k as u16),
                            INDENT,
                            Style::default(),
                        );
                    }
                }
            }
        }

        curr.shift -= INDENT_WIDTH as i32;
    };
}

impl StatefulWidget for &mut Tree {
    type State = View;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut View) {
        let stride = 3;

        state.ensure_cursor_within(area.height as i32, stride);

        let mut origin = Offset {
            shift: -state.offset.shift,
            scroll: -state.offset.scroll,
        };

        render_r(
            &self.root,
            &state.root,
            buf,
            &mut origin,
            area,
            Some(&state.cursor),
        );
    }
}

impl Tree {
    pub fn new(path: PathBuf) -> io::Result<Tree> {
        Ok(Tree {
            root: Node::new_root(path),
        })
    }
}
