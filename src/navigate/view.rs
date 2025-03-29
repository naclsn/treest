use std::fmt::{Debug, Formatter, Result as FmtResult};
use std::io::{Result as IoResult, Write};
use std::ops::Range;

use crate::navigate::options::Options;
use crate::navigate::{IndexPath, Message, Navigate};
use crate::providers::Provider;
use crate::tree::Node;

struct Appearance {
    branch: &'static str,
    indent: &'static str,
    branch_last: &'static str,
    indent_last: &'static str,
    scroll_before: &'static str,
    scroll_topmost: &'static str,
    scroll_top: &'static str,
    scroll_bar: &'static str,
    scroll_bot: &'static str,
    scroll_botmost: &'static str,
    scroll_after: &'static str,
}

const ASCII: Appearance = Appearance {
    branch: "|-- ",
    indent: "|   ",
    branch_last: "`-- ",
    indent_last: "    ",
    scroll_before: ": ",
    scroll_topmost: "% ",
    scroll_top: "+ ",
    scroll_bar: "# ",
    scroll_bot: "+ ",
    scroll_botmost: "% ",
    scroll_after: ": ",
};
const PRETTY: Appearance = Appearance {
    branch: "\u{251c}\u{2500}\u{2500} ",
    indent: "\u{2502}   ",
    branch_last: "\u{2514}\u{2500}\u{2500} ",
    indent_last: "    ",
    scroll_before: "\u{2502} ",
    scroll_topmost: "\u{2503} ",
    scroll_top: "\u{257d} ",
    scroll_bar: "\u{2503} ",
    scroll_bot: "\u{257f} ",
    scroll_botmost: "\u{2503} ",
    scroll_after: "\u{2502} ",
};

pub const MESSAGE_WINDOW_HEIGHT: usize = 12;

pub struct View {
    scroll: usize,
    cols: Range<usize>,
    rows: usize, // tree views all start at row 0

    line_mapping: Vec<IndexPath>,
    cursor_line: usize,
    total_height: usize,
}

struct RenderingState<'a> {
    node_path: Vec<&'a Node>,
    index_path: IndexPath,
    indent: Vec<&'static str>,

    visible_range: Range<usize>, // const
    total_height: &'a mut usize,
    lines: Vec<Option<String>>,

    line_mapping: &'a mut Vec<IndexPath>,
    was_cursor_line: usize, // const
    now_cursor_line: &'a mut usize,

    appearance: &'a Appearance,
}

impl Debug for RenderingState<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("RenderingState")
            .field(
                "node_path",
                &self
                    .node_path
                    .iter()
                    .map(|n| n.fragment)
                    .collect::<Vec<_>>(),
            )
            .field("index_path", &self.index_path)
            .field("indent", &self.indent)
            .field("visible_range", &self.visible_range)
            .field("total_height", &self.total_height)
            .field("lines", &self.lines)
            .field("line_mapping", &self.line_mapping)
            .field("was_cursor_line", &self.was_cursor_line)
            .field("now_cursor_line", &self.now_cursor_line)
            .finish()
    }
}

pub enum ViewJumpBy {
    Line,
    Mouse,
    HalfWin,
    Win,
}

impl View {
    pub fn new(col_offset: usize, cols: usize, rows: usize) -> Self {
        Self {
            scroll: 0,
            cols: col_offset..cols,
            rows,

            line_mapping: Vec::new(),
            cursor_line: 0,
            total_height: 0,
        }
    }

    pub fn path_for(&self, line: usize) -> Option<&IndexPath> {
        self.line_mapping.get(line)
    }

    fn jump_by(&self, by: ViewJumpBy) -> usize {
        use ViewJumpBy::*;
        match by {
            Line => return 1,
            Mouse => return 3,
            _ => (),
        }
        match by {
            HalfWin => self.rows / 2,
            Win => self.rows - 1,
            _ => unreachable!(),
        }
    }

    pub fn down(&mut self, by: ViewJumpBy) {
        let by = self.jump_by(by);
        if self.scroll + by < self.total_height {
            self.scroll += by;
        } else {
            self.scroll = self.total_height - 1;
        }
    }

    pub fn up(&mut self, by: ViewJumpBy) {
        let by = self.jump_by(by);
        if by < self.scroll {
            self.scroll -= by;
        } else {
            self.scroll = 0;
        }
    }

    /// Compute the lines needed to re-render the visible range of the tree.
    fn render_tree_range(
        &mut self,
        root: &Node,
        provider: &dyn Provider,
        cursor: &[usize],
        options: &Options,
    ) -> Vec<Option<String>> {
        fn inner<'a>(
            mut state: RenderingState<'a>,
            node: &'a Node,
            provider: &dyn Provider,
            cursor: &'a [usize],
        ) -> RenderingState<'a> {
            state.node_path.push(node);

            if state.visible_range.contains(state.total_height) {
                let at = state.lines.len();

                let was_cursor = at == state.was_cursor_line;
                let now_cursor = cursor == &state.index_path[..];

                let need_push = state.line_mapping.len() == at;
                let changed = need_push
                    || was_cursor
                    || now_cursor
                    || state.index_path != state.line_mapping[at];

                if !changed {
                    state.lines.push(None);
                } else {
                    if need_push {
                        state.line_mapping.push(state.index_path.clone());
                    } else {
                        state.line_mapping[at] = state.index_path.clone();
                    }

                    let mut line = state.indent.join("");
                    if node.is_marked() {
                        line.push_str(" \x1b[4m");
                    }
                    if now_cursor {
                        line.push_str("\x1b[7m");
                        *state.now_cursor_line = at;
                    }
                    line.push_str(&provider.display(&state.node_path[..].into()));

                    state.lines.push(Some(line));
                }
            }

            *state.total_height += 1;

            if !node.is_folded() {
                if let &[ref init_children @ .., last_child] = &node.children().unwrap()[..] {
                    if init_children.is_empty() {
                        todo!();
                        // child occupies same line
                        *state.total_height -= 1;
                    }

                    if let Some(branch) = state.indent.last_mut() {
                        *branch = if std::ptr::eq(state.appearance.branch, *branch) {
                            state.appearance.indent
                        } else {
                            state.appearance.indent_last
                        };
                    }
                    state.indent.push(state.appearance.branch);
                    state.index_path.push(0);

                    {
                        let index = state.index_path.len() - 1;
                        for child in init_children.iter() {
                            state = inner(state, child, provider, cursor);
                            state.index_path[index] += 1;
                        }

                        *state.indent.last_mut().unwrap() = state.appearance.branch_last;
                        state = inner(state, last_child, provider, cursor);
                    }

                    state.index_path.pop();
                    state.indent.pop();
                    if let Some(indent) = state.indent.last_mut() {
                        *indent = if std::ptr::eq(state.appearance.indent, *indent) {
                            state.appearance.branch
                        } else {
                            state.appearance.branch_last
                        };
                    }
                }
            }

            state.node_path.pop();
            state
        }

        self.total_height = 0;

        let state = RenderingState {
            node_path: Vec::new(),
            index_path: IndexPath::default(),
            indent: Vec::new(),

            visible_range: self.scroll..self.scroll + self.rows,
            total_height: &mut self.total_height,
            lines: Vec::new(),

            line_mapping: &mut self.line_mapping,
            was_cursor_line: self.cursor_line,
            now_cursor_line: &mut self.cursor_line,

            appearance: match options.appearance.as_str() {
                "pretty" => &PRETTY,
                "ascii" => &ASCII,
                _ => unreachable!(),
            },
        };

        let mut lines = inner(state, root, provider, cursor).lines;

        let render_height = lines.len();
        if render_height < self.line_mapping.len() {
            // the previous render was bigger than the new one:
            // fill rendering with just enough empty lines to clear the diff
            lines.resize(self.line_mapping.len(), Some(String::new()));
        }
        self.line_mapping.truncate(render_height);

        lines
    }

    /// Re-render any changed line.
    ///
    /// Cursor positions before and after are unspecified.
    pub fn render(
        &mut self,
        f: &mut impl Write,
        root: &Node,
        provider: &dyn Provider,
        cursor: &[usize],
        options: &Options,
    ) -> IoResult<()> {
        let lines = self.render_tree_range(root, provider, cursor, options);

        write!(f, "\x1b[;{}H", self.cols.start + 1)?;
        for (off, line) in lines.iter().enumerate() {
            // None truly means "don't touch the line, it's good as is"
            // Some means replace with this, (TODO) clearing existing as needed
            if let Some(line) = line {
                // TODO: potential optimizations:
                //      * trim leading spaces (increment col as needed)
                //      * use '\r\n' when col is 0
                // TODO: trim line to available width, cache used width for later clearing
                write!(f, "\x1b[{};{}H", off + 1, self.cols.start + 1)?;
                write!(f, "\x1b[K")?; // temporary hard full-line clear
                write!(f, "{line}\x1b[m")?;
            }
        }

        Ok(())
    }
}

impl Navigate {
    pub fn render(&mut self, f: &mut impl Write) -> IoResult<()> {
        self.view.render(
            f,
            &self.tree,
            self.provider.as_ref(),
            &self.cursor.1[..self.cursor.0],
            &self.options,
        )
    }
}
