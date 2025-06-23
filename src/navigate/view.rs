use std::io::{Result as IoResult, Write};
use std::ops::Range;

use crate::navigate::options::GlobalOptionsRef;
use crate::navigate::{IndexPath, Navigate};
use crate::provider::Provider;
use crate::terminal;
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

#[derive(Default)]
pub struct View {
    scroll: usize,
    line_mapping: Vec<IndexPath>,
    cursor_line: usize,
    visible_height: usize,
    total_height: usize,

    cols: Range<usize>,
    rows: usize,               // tree views all start at row 0
    has_multiple_spaces: bool, // indicates if CSI K can be used to clear lines
}

/// Parts of the `Space` needed by the `View` for rendering.
pub struct ViewSpaceSubset<'a> {
    pub root: &'a Node,
    pub provider: &'a dyn Provider,
    pub cursor: &'a [usize],
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
    //singlechildline: bool,
}

pub enum ViewJumpBy {
    Line,
    Mouse,
    HalfWin,
    Win,
}

impl ViewJumpBy {
    pub fn new_by(amount: &str) -> Self {
        match amount {
            "line" => ViewJumpBy::Line,
            "win" => ViewJumpBy::Win,
            "halfwin" => ViewJumpBy::HalfWin,
            "mouse" => ViewJumpBy::Mouse,
            _ => unreachable!(),
        }
    }

    fn jump_by(&self, win: usize) -> usize {
        use ViewJumpBy::*;
        match self {
            Line => return 1,
            Mouse => return 3,
            _ => (),
        }
        match self {
            HalfWin => win / 2,
            Win => win - 1,
            _ => unreachable!(),
        }
    }

    pub fn down(&self, scroll: &mut usize, win: usize, top: usize) {
        let by = self.jump_by(win);
        if *scroll + by < top {
            *scroll += by;
        } else {
            *scroll = top.saturating_sub(1);
        }
    }

    pub fn up(&self, scroll: &mut usize, win: usize, bot: usize) {
        let by = self.jump_by(win);
        if bot + by < *scroll {
            *scroll -= by;
        } else {
            *scroll = bot;
        }
    }

    pub fn view_down(&self, view: &mut View) {
        self.down(&mut view.scroll, view.visible_height, view.total_height)
    }

    pub fn view_up(&self, view: &mut View) {
        self.up(&mut view.scroll, view.visible_height, 0)
    }
}

impl View {
    pub fn update(&mut self, cols: Range<usize>, rows: usize, has_multiple_spaces: bool) {
        self.cols = cols;
        self.rows = rows;
        self.has_multiple_spaces = has_multiple_spaces;
    }

    pub fn path_for(&self, line: usize) -> Option<&IndexPath> {
        self.line_mapping.get(line)
    }

    /// Compute the lines needed to re-render the visible range of the tree.
    fn render_tree_range(
        &mut self,
        space: ViewSpaceSubset,
        visible_range: Range<usize>,
        options: &GlobalOptionsRef,
    ) -> Vec<Option<String>> {
        fn inner<'a>(
            mut state: RenderingState<'a>,
            node: &'a Node,
            provider: &dyn Provider,
            cursor: &'a [usize],
            is_node_sch: bool,
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

                    if is_node_sch {
                        todo!("'singlechildline'");
                    } else {
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
            }

            *state.total_height += 1;

            if !node.is_folded() {
                let children: Vec<_> = node.children().unwrap().collect();
                if let &[ref init_children @ .., last_child] = &children[..] {
                    // TODO: singlechildline
                    /*if init_children.is_empty() && state.singlechildline {
                        // child occupies same line
                        *state.total_height -= 1;

                        state.index_path.push(0);
                        state = inner(state, last_child, provider, cursor, true);
                        state.index_path.pop();
                    } else*/
                    {
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
                                state = inner(state, child, provider, cursor, false);
                                state.index_path[index] += 1;
                            }

                            *state.indent.last_mut().unwrap() = state.appearance.branch_last;
                            state = inner(state, last_child, provider, cursor, false);
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
            }

            state.node_path.pop();
            state
        }

        self.visible_height = visible_range.len();
        self.total_height = 0;

        let state = {
            let options = options.lock();

            RenderingState {
                node_path: Vec::new(),
                index_path: IndexPath::default(),
                indent: Vec::new(),

                visible_range,
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
                //singlechildline: options.singlechildline,
            }
        };

        let mut lines = inner(state, space.root, space.provider, space.cursor, false).lines;

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
    /// If the flag `has_multiple_spaces` is not set, lines can be cleared with CSI K (3 bytes).
    /// Otherwise it has to be more meticulous about it.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        f: &mut impl Write,
        force: bool,
        space: ViewSpaceSubset,
        options: &GlobalOptionsRef,
    ) -> IoResult<()> {
        if force {
            self.line_mapping.clear();
        }

        let range = self.scroll..self.scroll + self.rows;
        let lines = self.render_tree_range(space, range, options);

        write!(f, "\x1b[;{}H", self.cols.start + 1)?;
        for (off, line) in lines.iter().enumerate() {
            // None truly means "don't touch the line, it's good as is"
            // Some means replace with this, (TODO) clearing existing as needed
            if let Some(line) = line {
                // TODO: potential optimizations:
                //      * trim leading spaces (increment col as needed)
                //      * use '\r\n' when col is 0
                write!(f, "\x1b[{};{}H", off + 1, self.cols.start + 1)?;
                if self.has_multiple_spaces {
                    // TODO: trim line to available width, cache used width for clearing
                    write!(f, "{}", " ".repeat(self.cols.len()))?;
                } else {
                    write!(f, "\x1b[K")?;
                }
                write!(f, "{line}\x1b[m")?;
            }
        }

        Ok(())
    }
}

impl Navigate {
    /// Re-render the things.
    ///
    /// Essentially:
    ///     * clear just message window to end of screen;
    ///     * update tree(s) (fully re-render if `force`);
    ///     * render any message;
    ///     * render any prompt line.
    ///
    /// Cursor positions before and after are unspecified.
    pub fn render(&mut self, f: &mut impl Write, force: bool) -> IoResult<()> {
        let (term_col, term_row) = terminal::size()
            .map(|t| (t.col as usize, t.row as usize))
            .unwrap_or((80, 24));

        // retrieve all options needed early and release lock right away
        let (messageheight, appearance, messagescrollbar) = {
            let options = self.options.make_ref();
            let options = options.lock();
            (
                options.messageheight,
                options.appearance.clone(),
                options.messagescrollbar,
            )
        };

        // clear message from previous render
        write!(f, "\x1b[{}H", term_row - self.message.previous_height - 1)?;
        if self.input.get_prompt().is_none() {
            write!(f, "\x1b[J")?;
        } else {
            // if there is a prompt, can't just clear to end of screen, have to clear each line
            write!(f, "{}", "\x1b[K\n".repeat(self.message.previous_height))?;
        }
        let len = self.message.lines.len();
        let msh = messageheight as usize;
        let height = std::cmp::min(len, msh);
        self.message.previous_height = height;

        let each_avail_col = term_col / self.spaces.len();
        let avail_rows = term_row - height - 2;
        let has_multiple_spaces = 1 < self.spaces.len();
        for (k, space) in self.spaces.iter_mut().enumerate() {
            let avail_cols = k * each_avail_col..(k + 1) * each_avail_col;
            let mut space = space.lock().unwrap();
            space.view_update(avail_cols, avail_rows, has_multiple_spaces);
            // XXX/TODO: main thread will repeatedly acquire lock on options
            space.view_render(f, force)?;
        }

        // render message and -- {} lines -- or breadcrumbs (message is always re-rendered)
        if !self.message.lines.is_empty() {
            write!(f, "\x1b[{}H", term_row - height - 1)?;

            let top = self.message.scroll * msh / len;
            let bot = std::cmp::min(top + msh * msh / len, height - 1);

            let appearance = match appearance.as_str() {
                "pretty" => PRETTY,
                "ascii" => ASCII,
                _ => unreachable!(),
            };

            let visible_range = self.message.scroll..self.message.scroll + height;
            for (k, line) in self.message.lines[visible_range].iter().enumerate() {
                if messagescrollbar {
                    let sb = if k < top {
                        appearance.scroll_before
                    } else if top == k && 0 == top {
                        appearance.scroll_topmost
                    } else if bot == k && bot < msh {
                        appearance.scroll_botmost
                    } else if top == k {
                        appearance.scroll_top
                    } else if k < bot {
                        appearance.scroll_bar
                    } else if bot == k {
                        appearance.scroll_bot
                    } else {
                        appearance.scroll_after
                    };
                    write!(f, "{sb}")?;
                }
                // TODO: trim line to available width
                write!(f, "{line}\r\n")?;
            }

            // don't render that if there is a completion session
            if self.input.get_prompt().is_none_or(|p| !p.has_compl()) {
                write!(
                    f,
                    "-- {} line{} --\r\n",
                    self.message.lines.len(),
                    if 1 == self.message.lines.len() {
                        ""
                    } else {
                        "s"
                    }
                )?;
            }
        } else {
            write!(f, "\x1b[{}H\x1b[K", term_row - 1)?;
            // don't render that if there is a completion session
            if self.input.get_prompt().is_none_or(|p| !p.has_compl()) {
                // show active space breadcrumbs
                let space = self.space();
                let path = space.tree.resolve(space.cursor());
                write!(f, "{}\r\n", space.provider.breadcrumbs(&path[..].into()))?;
            }
        }

        match (force, self.input.get_prompt()) {
            (true, Some(prompt)) => prompt.render(f)?,
            (_, None) => write!(f, "{}", terminal::keyseqstr(self.input.get_pending()))?,
            _ => (),
        }
        Ok(())
    }
}
