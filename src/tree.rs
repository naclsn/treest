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
const INDENT_WIDTH: u16 = 4;

fn render_name(
    tree_node: &Node,
    state_node: &State,
    buf: &mut Buffer,
    (indent, line): (u16, u16),
    area: Rect,
    is_cursor: bool,
) -> usize {
    if area.width <= indent {
        return 0;
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
        c.width()
    } else {
        let ext = tree_node.extension().unwrap_or("");
        let cut = 1 + ext.len() + deco.len() + raw_prefix.width() + raw_suffix.width();
        let visible = file_name.chars().take(avail_len - cut).collect::<String>();

        let c = Spans::from(vec![
            raw_prefix,
            Span::styled(&visible, sty),
            Span::styled("\u{2026}", sty),
            Span::styled(ext, sty),
            Span::raw(deco),
            raw_suffix,
        ]);

        buf.set_spans(area.x + indent, area.y + line, &c, area.width - indent);
        c.width()
    }
}

fn render_r(
    state_node: &State,
    tree_node: &Node,
    buf: &mut Buffer,
    curr: &mut Offset,
    bump: i32,
    area: Rect,
    cursor_path: Option<&[usize]>,
) {
    // this node
    let mut name_width = 0;
    if 0 <= curr.shift && 0 <= curr.scroll {
        name_width = render_name(
            tree_node,
            state_node,
            buf,
            ((curr.shift + bump) as u16, curr.scroll as u16),
            area,
            if let Some(v) = cursor_path {
                v.is_empty()
            } else {
                false
            },
        ) as i32;
    }
    curr.scroll += 1;

    // recurse
    if state_node.unfolded && !state_node.children.is_empty() {
        let count = state_node.children.len();
        let bump = if 1 < count {
            curr.shift += INDENT_WIDTH as i32;
            0
        } else {
            curr.scroll -= 1;
            bump + name_width
        };

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

            let p_indent = curr.shift;
            let p_line = curr.scroll;
            render_r(
                state_node,
                tree_node,
                buf,
                curr,
                bump,
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

            if INDENT_WIDTH as i32 <= curr.shift && 0 <= curr.scroll {
                buf.set_string(
                    area.x + (p_indent as u16) - INDENT_WIDTH,
                    area.y + (p_line as u16),
                    if is_last { BRANCH_LAST } else { BRANCH },
                    Style::default(),
                );
            }

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

        if 1 < count {
            curr.shift -= INDENT_WIDTH as i32;
        }
    };
}

impl StatefulWidget for &Tree {
    type State = View;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut View) {
        let stride = 3;

        state.ensure_cursor_within(area.height as i32, stride);

        let view_offset = state.view_offset();
        let mut origin = Offset {
            shift: -view_offset.shift,
            scroll: -view_offset.scroll,
        };

        render_r(
            &state.root,
            &self.root,
            buf,
            &mut origin,
            0,
            area,
            Some(state.cursor_path()),
        );
    }
}

impl Tree {
    pub fn new(path: PathBuf) -> io::Result<Tree> {
        Ok(Tree {
            root: Node::new_root(path)?,
        })
    }

    /// re-create the tree from the file system; even
    /// though a Tree is lazy, this is NOT a no-op: it
    /// tries to re-load the nodes that previously where
    /// @ret the previous root node
    /// @see also `View::renew_root`
    pub fn renew(&self) -> io::Result<Tree> {
        Ok(Tree {
            root: self.root.renew()?,
        })
    }
}
