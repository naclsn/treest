use std::fmt::{Debug, Formatter, Result as FmtResult};
use std::io::{Result as IoResult, Write};
use std::ops::Range;

use crate::navigate::{IndexPath, Message, Navigate};
use crate::providers::Provider;
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

pub const MESSAGE_WINDOW_HEIGHT: usize = 12;

pub struct View {
    scroll: usize,
    total: Range<usize>,
    invalidated: Option<Range<usize>>,
    term_col: usize,
    term_row: usize,
    line_mapping: Vec<IndexPath>,
}

struct RenderingState<'a> {
    node_path: Vec<&'a Node>,
    index_path: IndexPath,
    indent: Vec<&'static str>,

    range: &'a Range<usize>,
    cursor: usize,
    lines: Vec<String>,

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
            .field("range", &self.range)
            .field("cursor", &self.cursor)
            .field("lines", &self.lines)
            .finish()
    }
}

pub enum ViewJumpBy {
    Line,
    Mouse,
    HalfWin,
    Win,
}

impl Default for View {
    fn default() -> Self {
        Self {
            scroll: 0,
            total: 0..0,
            invalidated: None,
            term_col: 80,
            term_row: 24,
            line_mapping: Vec::new(),
        }
    }
}

impl View {
    ///
    /// `view.visible` is called every frame, so we take the opportunity to
    /// fetch and cache the terminal size again.
    fn visible(&mut self) -> Range<usize> {
        if let Ok(term_size) = terminal::size() {
            self.term_col = term_size.col as usize;
            self.term_row = term_size.row as usize;
        }
        self.scroll..self.scroll + self.term_row - 2
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
            HalfWin => self.term_row / 2,
            Win => self.term_row - 1,
            _ => unreachable!(),
        }
    }

    pub fn down(&mut self, by: ViewJumpBy) {
        let by = self.jump_by(by);
        let end = self.total.end;
        if self.scroll + by < end {
            self.scroll += by;
        } else {
            self.scroll = end - 1;
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
}

impl Navigate {
    // TODO: (almost everywhere) check and trim at terminal width (visible) characters
    pub fn render(&mut self, f: &mut impl Write) -> IoResult<()> {
        write!(f, "\x1b[H\x1b[J")?;

        let cursor = self.tree.resolve_node(self.cursor());

        let visible = self.view.visible();
        let mut line_mapping = vec![IndexPath::default(); visible.len()];

        let mut current = 0;
        self.render_at(
            f,
            (&mut vec![&self.tree], &mut IndexPath::default()),
            cursor,
            String::new(),
            (&mut current, &visible, &mut line_mapping),
        )?;

        line_mapping.truncate(current);
        self.view.line_mapping = line_mapping;
        self.view.total.end = current;

        if current < visible.end {
            write!(f, "{}", "\n".repeat(visible.end - current))?;
        }

        if let Some(Message { lines, offset, .. }) = &self.message {
            let len = std::cmp::min(lines.len(), MESSAGE_WINDOW_HEIGHT);
            write!(f, "\x1b[{}A", len)?;

            let top = offset * MESSAGE_WINDOW_HEIGHT / lines.len();
            let bot = std::cmp::min(
                top + MESSAGE_WINDOW_HEIGHT * MESSAGE_WINDOW_HEIGHT / lines.len(),
                len - 1,
            );
            let appearance = match self.options.appearance.as_str() {
                "pretty" => PRETTY,
                _ => ASCII,
            };

            for (k, line) in lines[*offset..*offset + len].iter().enumerate() {
                let sb = if k < top {
                    appearance.scroll_before
                } else if top == k && 0 == top {
                    appearance.scroll_topmost
                } else if bot == k && bot < MESSAGE_WINDOW_HEIGHT {
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

                write!(f, "{sb}{line}\r\n")?;
            }

            if MESSAGE_WINDOW_HEIGHT < lines.len() {
                write!(f, "-- More ({} lines) --\r\n", lines.len())?;
            } else {
                write!(f, "-- (End) --\r\n")?;
            }
        } else {
            let path = self.tree.resolve(self.cursor());
            write!(f, "{}\r\n", self.provider.breadcrumbs(&path[..].into()))?;
            write!(f, "{}", terminal::keyseqstr(self.input.get_pending()))?;
        }

        Ok(())
    }

    fn render_at(
        &self,
        f: &mut impl Write,
        (at, k_at): (&mut Vec<&Node>, &mut IndexPath),
        cursor: &Node,
        indent: String,
        (current, visible, line_mapping): (&mut usize, &Range<usize>, &mut [IndexPath]),
    ) -> IoResult<()> {
        let node = at.last().unwrap();

        if visible.contains(current) {
            if node.is_marked() {
                write!(f, " \x1b[4m")?;
            }
            if std::ptr::eq(cursor, *node) {
                write!(f, "\x1b[7m")?;
            }
            let frag = self.provider.display(&at[..].into());
            write!(f, "{frag}\x1b[m")?;
            line_mapping[*current - visible.start] = k_at.clone();
        }

        if node.is_folded() {
            if visible.contains(current) {
                write!(f, "\r\n")?;
            }
            *current += 1;
            return Ok(());
        }

        let children = node.children().unwrap();

        if children.is_empty() {
            if visible.contains(current) {
                write!(f, "\r\n")?;
            }
            *current += 1;
            return Ok(());
        }

        if 1 == children.len() {
            at.push(children[0]);
            k_at.push(0);
            let r = self.render_at(
                f,
                (at, k_at),
                cursor,
                indent,
                (current, visible, line_mapping),
            );
            at.pop();
            k_at.pop();
            r
        } else {
            if visible.contains(current) {
                write!(f, "\r\n")?;
            }
            *current += 1;

            let appearance = match self.options.appearance.as_str() {
                "pretty" => PRETTY,
                _ => ASCII,
            };

            let child_count = children.len();
            for (k, it) in children[..child_count - 1].iter().enumerate() {
                if visible.contains(current) {
                    write!(f, "{indent}{}", appearance.branch)?;
                }
                at.push(it);
                k_at.push(k);
                self.render_at(
                    f,
                    (at, k_at),
                    cursor,
                    format!("{indent}{}", appearance.indent),
                    (current, visible, line_mapping),
                )?;
                at.pop();
                k_at.pop();
            }

            // last iteration unrolled (uses `appearance.[..]_last`)
            if visible.contains(current) {
                write!(f, "{indent}{}", appearance.branch_last)?;
            }
            at.push(children[child_count - 1]);
            k_at.push(child_count - 1);
            let r = self.render_at(
                f,
                (at, k_at),
                cursor,
                format!("{indent}{}", appearance.indent_last),
                (current, visible, line_mapping),
            );
            at.pop();
            k_at.pop();

            r
        }
    }

    /// Render the lines for a view range of the tree.
    ///
    /// Range is a 0-base in-view half-open range: start of 0 and `view.scroll` to initial 0 means
    /// the root is shown in the first returned line, end of 1 would mean it's the only line
    /// returned.
    pub fn render_tree_range(&mut self, range: Range<usize>) -> Vec<String> {
        // The `Result` type is used for the short-circuit syntax:
        // * `Err(lines)` means it's done rendering and that's the actual result;
        // * `Ok(state)` means range couldn't be fulfill in this iteration.
        // It is (for now) a panic when the top level result is `Ok` (the range was too big).
        fn inner<'a>(
            mut state: RenderingState<'a>,
            provider: &Box<dyn Provider>,
            node: &'a Node,
        ) -> Result<RenderingState<'a>, Vec<String>> {
            // +1 because state.lines always contains the line this iteration planned to maybe fill
            if state.range.len() + 1 == state.lines.len() {
                state.lines.pop();
                return Err(state.lines); // done (enough lines to fulfill `range`)
            }

            state.node_path.push(node);

            // only check start; it should not be possible to reach end because of len check above
            if state.range.start <= state.cursor {
                let line = state.lines.last_mut().unwrap();
                if node.is_marked() {
                    line.push_str(" \x1b[4m");
                }
                // TODO
                //if cursor {
                //    line.push_str("\x1b[7m");
                //}
                line.push_str(&provider.display(&state.node_path[..].into()));
            }

            state.cursor += 1;

            if !node.is_folded() {
                if let &[ref init_children @ .., last_child] = &node.children().unwrap()[..] {
                    if init_children.is_empty() {
                        // child occupies same line
                        state.cursor -= 1;
                    }

                    if !init_children.is_empty() {
                        if let Some(branch) = state.indent.last_mut() {
                            *branch = if std::ptr::eq(state.appearance.branch, *branch) {
                                state.appearance.indent
                            } else {
                                state.appearance.indent_last
                            };
                        }
                        state.indent.push(state.appearance.branch)
                    }
                    state.index_path.push(0);

                    {
                        let index = state.index_path.len() - 1;
                        for child in init_children.into_iter() {
                            // branch in loop: need to check on every iteration, better than alloc in loop
                            if state.range.start <= state.cursor {
                                state.lines.push(state.indent.join(""));
                            }
                            state = inner(state, provider, child)?;
                            state.index_path[index] += 1;
                        }

                        if !init_children.is_empty() && state.range.start <= state.cursor {
                            *state.indent.last_mut().unwrap() = state.appearance.branch_last;
                            state.lines.push(state.indent.join(""));
                        }
                        state = inner(state, provider, last_child)?;
                    }

                    if state.range.len() == state.lines.len() {
                        return Err(state.lines); // done (just reached it)
                    }

                    state.index_path.pop();
                    if !init_children.is_empty() {
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
            Ok(state)
        }

        let mut state = RenderingState {
            node_path: Vec::new(),
            index_path: IndexPath::default(),
            indent: Vec::new(),

            range: &range,
            cursor: 0,
            lines: Vec::new(),

            appearance: match self.options.appearance.as_str() {
                "pretty" => &PRETTY,
                "ascii" => &ASCII,
                _ => unreachable!(),
            },
        };

        // when inner is called, if it needs a line it expects a line;
        // only case where top-level needs to ensure a line is available:
        if 0 == state.range.start {
            state.lines.push(String::new());
        }

        inner(state, &self.provider, &self.tree).unwrap_err()
    }
}

/// Skip rendering until root-relative display line `target`.
///
/// That is: the returned `RenderingState` correspond to the state *at* rendering (1-base) target.
/// "root-relative" means it starts counting from the root (no scroll offset involved). As such
/// `target` should be at least 1 (there is no "display line 0") and a target of exactly 1 is the
/// rendering of the root itself.
/*fn skip_to<'a>(root: &'a Node, target: usize) -> RenderingState<'a> {
    if 0 == target {
        panic!("skip_to called with target of 0");
    }

    // The `Result` type is used for the short-circuit syntax:
    // * Err(r) means it reached the target and r is correct
    // * Ok((r, current)) means it didn't reach the target and stopped at current
    fn inner<'a>(
        (mut state, mut current): (RenderingState<'a>, usize),
        target: usize,
        node: &'a Node,
    ) -> Result<(RenderingState<'a>, usize), RenderingState<'a>> {
        current += 1;
        if target == current {
            state.node_path.push(node);
            return Err(state); // found
        }

        if !node.is_folded() {
            let mut children = node.children().unwrap();
            if !children.is_empty() {
                let single = 1 == children.len();
                if single {
                    // if I didn't make it, don't count me: the first single child in the chain
                    // that has =0 or >1 children will count for the line
                    current -= 1;
                } else {
                    state.indent.push("todo")
                }
                state.node_path.push(node);

                let last_child = children.pop().unwrap();
                let last_index = children.len();

                for (index, child) in children.into_iter().enumerate() {
                    state.index_path.push(index);
                    (state, current) = inner((state, current), target, child)?;
                    state.index_path.pop();
                }

                if !single {
                    *state.indent.last_mut().unwrap() = "todo";
                }

                state.index_path.push(last_index);
                (state, current) = inner((state, current), target, last_child)?;
                state.index_path.pop();

                state.node_path.pop();
                if !single {
                    state.indent.pop();
                }
            }
        }

        Ok((state, current)) // cannot find
    }

    let r = RenderingState {
        node_path: Vec::new(),
        index_path: IndexPath::default(),
        indent: Vec::new(),
    };
    match inner((r, 0), target, root) {
        Err(r) => r,
        #[cfg(not(test))]
        Ok((_, current)) => panic!("skip_to could not reach target: {current}/{target}"),
        #[cfg(test)]
        Ok((r, _)) => r,
    }
}*/

#[cfg(test)]
#[test]
fn test_something() {
    use crate::providers::Fragment;
    #[derive(Clone, Debug, PartialEq)]
    struct RS {
        node_path: Vec<Fragment>,
        index_path: IndexPath,
        depth: usize,
    }
    impl From<RenderingState<'_>> for RS {
        fn from(value: RenderingState) -> Self {
            Self {
                node_path: value.node_path.into_iter().map(|n| n.fragment).collect(),
                index_path: value.index_path,
                depth: value.indent,
            }
        }
    }

    assert_eq!(
        RS::from(skip_to(&crate::make_test_tree!(0 true false -), 1,)),
        RS {
            node_path: vec![Fragment(0)].into(),
            index_path: vec![].into(),
            depth: 0,
        },
    );

    assert_eq!(
        RS::from(skip_to(
            &crate::make_test_tree!(
                1 false false [ // this one
                    11 true false -
                    12 true false -
                    13 true false - ]
            ),
            1,
        )),
        RS {
            node_path: vec![Fragment(1)].into(),
            index_path: vec![].into(),
            depth: 0,
        },
    );

    assert_eq!(
        RS::from(skip_to(
            &crate::make_test_tree!(
                1 false false [
                    11 true false -
                    12 true false - // this one
                    13 true false - ]
            ),
            3,
        )),
        RS {
            node_path: vec![Fragment(1), Fragment(12)].into(),
            index_path: vec![1].into(),
            depth: 1,
        },
    );

    assert_eq!(
        RS::from(skip_to(
            &crate::make_test_tree!(
                1 false false [
                    11 true false -
                    12 true false -
                    13 true false - ]
                // oor
            ),
            5,
        )),
        RS {
            node_path: vec![].into(),
            index_path: vec![].into(),
            depth: 0,
        },
    );

    assert_eq!(
        RS::from(skip_to(
            &crate::make_test_tree!(
                1 false false [ 11 false false [ 111 true false - ] ]
                // oor
            ),
            2,
        )),
        RS {
            node_path: vec![].into(),
            index_path: vec![].into(),
            depth: 0,
        },
    );

    assert_eq!(
        RS::from(skip_to(
            &crate::make_test_tree!(
                1 false false [ 11 false false [ 111 false false [
                            1111 true false -
                            1112 true false - // this one
                            1113 true false - ] ] ]
            ),
            3,
        )),
        RS {
            node_path: vec![Fragment(1), Fragment(11), Fragment(111), Fragment(1112)].into(),
            index_path: vec![0, 0, 1].into(),
            depth: 1,
        },
    );

    assert_eq!(
        RS::from(skip_to(
            &crate::make_test_tree!(
                1 false false [
                    11 true false -
                    12 false false [
                        121 true false -
                        122 true false - // this one
                        123 true false - ] ]
            ),
            5,
        )),
        RS {
            node_path: vec![Fragment(1), Fragment(12), Fragment(122)].into(),
            index_path: vec![1, 1].into(),
            depth: 2,
        },
    );

    assert_eq!(
        RS::from(skip_to(
            &crate::make_test_tree!(
                1 false false [
                    11 false false [
                        111 true false -
                        111 true false - ]
                    12 false false [
                        121 true false - // this one
                        122 true false -
                        123 true false - ] ]
            ),
            6,
        )),
        RS {
            node_path: vec![Fragment(1), Fragment(12), Fragment(121)].into(),
            index_path: vec![1, 0].into(),
            depth: 2,
        },
    );
}
