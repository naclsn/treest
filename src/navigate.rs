use std::cell::RefCell;
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::ops::Range;

use crate::terminal;
use crate::tree::{NodeRef, Provider, ProviderExt, Tree};

pub struct Navigate<P: Provider> {
    tree: Tree<P>,
    cursor: NodeRef,
    state: State,
    view: View,
}

pub enum State {
    Continue,
    Pending(Vec<u8>),
    Quit,
}

struct View {
    scroll: usize,
    total: RefCell<Range<usize>>,
}
enum ViewJumpBy {
    Line,
    Mouse,
    HalfWin,
    Win,
}

impl View {
    fn visible(&self) -> Range<usize> {
        let row = terminal::size().unwrap_or((24, 80)).0 as usize;
        self.scroll..self.scroll + row - 2
    }

    fn jump_by(&self, by: ViewJumpBy) -> usize {
        use ViewJumpBy::*;
        match by {
            Line => return 1,
            Mouse => return 3,
            _ => (),
        }
        let row = terminal::size().unwrap_or((24, 80)).0 as usize;
        match by {
            HalfWin => row / 2,
            Win => row - 1,
            _ => unreachable!(),
        }
    }

    fn down(&mut self, by: ViewJumpBy) {
        let by = self.jump_by(by);
        let end = self.total.borrow().end;
        if self.scroll < end - by {
            self.scroll += by;
        } else {
            self.scroll = end - 1;
        }
    }

    fn up(&mut self, by: ViewJumpBy) {
        let by = self.jump_by(by);
        if by < self.scroll {
            self.scroll -= by;
        } else {
            self.scroll = 0;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Direction {
    Prev,
    Next,
}

impl Direction {
    pub fn go(&self, k: usize) -> usize {
        match self {
            Direction::Prev => k - 1,
            Direction::Next => k + 1,
        }
    }

    pub fn go_sat(&self, k: usize, m: usize) -> usize {
        match self {
            Direction::Prev if 0 != k => k - 1,
            Direction::Next if k < m - 1 => k + 1,
            _ => k,
        }
    }

    pub fn go_wrap(&self, k: usize, m: usize) -> usize {
        match self {
            Direction::Prev if 0 == k => m - 1,
            Direction::Next if k == m - 1 => 0,
            _ => self.go(k),
        }
    }
}

impl<P: Provider> Navigate<P> {
    pub fn new(provider: P) -> Self {
        let mut tree = Tree::new(provider);
        let cursor = tree.root();
        tree.unfold_at(cursor);
        Self {
            tree,
            cursor,
            state: State::Continue,
            view: View {
                scroll: 0,
                total: (0..0).into(),
            },
        }
    }

    pub fn feed(&mut self, key: u8) -> &State {
        match self.state {
            State::Continue => match key {
                /* ^B */ 0x02 => self.view.up(ViewJumpBy::Win),
                /* ^D */ 0x04 => self.view.down(ViewJumpBy::HalfWin),
                /* ^E */ 0x05 => self.view.down(ViewJumpBy::Line),
                /* ^F */ 0x06 => self.view.down(ViewJumpBy::Win),
                /* ^J */ 0x0a => self.sibling_wrap(Direction::Next),
                /* ^K */ 0x0b => self.sibling_wrap(Direction::Prev),
                /* ^U */ 0x15 => self.view.up(ViewJumpBy::HalfWin),
                /* ^Y */ 0x19 => self.view.up(ViewJumpBy::Line),
                /* ^[ */ 0x1b => self.state = State::Pending(vec![key]),
                b'0' => self.root(),
                b'H' => self.fold(),
                b'L' => self.unfold(),
                b'h' => self.leave(),
                b'j' => self.sibling_sat(Direction::Next),
                b'k' => self.sibling_sat(Direction::Prev),
                b'l' => self.enter(),
                b'q' => self.state = State::Quit,
                b' ' => self.toggle_mark(),
                _ => (),
            },
            State::Pending(ref mut v) => {
                if 0x07 /* ^G */ == key {
                    self.state = State::Continue;
                } else {
                    v.push(key);
                }
            }
            State::Quit => panic!("should have quitted, but was called again!"),
        }
        &self.state
    }

    pub fn root(&mut self) {
        self.cursor = self.tree.root();
    }

    pub fn fold(&mut self) {
        self.tree.fold_at(self.cursor)
    }

    pub fn unfold(&mut self) {
        self.tree.unfold_at(self.cursor)
    }

    pub fn sibling_sat(&mut self, dir: Direction) {
        let siblings = self
            .tree
            .at(self.tree.at(self.cursor).parent())
            .children()
            .unwrap();
        if let Some(me) = siblings.iter().position(|c| self.cursor == *c) {
            self.cursor = siblings[dir.go_sat(me, siblings.len())];
        }
    }

    pub fn sibling_wrap(&mut self, dir: Direction) {
        let siblings = self
            .tree
            .at(self.tree.at(self.cursor).parent())
            .children()
            .unwrap();
        if let Some(me) = siblings.iter().position(|c| self.cursor == *c) {
            self.cursor = siblings[dir.go_wrap(me, siblings.len())];
        }
    }

    pub fn enter(&mut self) {
        self.unfold();
        if let Some(child) = self
            .tree
            .at(self.cursor)
            .children()
            .and_then(|cs| cs.iter().next())
        {
            self.cursor = *child;
        }
    }

    pub fn leave(&mut self) {
        self.cursor = self.tree.at(self.cursor).parent();
    }

    pub fn toggle_mark(&mut self) {
        self.tree.toggle_mark_at(self.cursor);
    }
}

const BRANCH: &str = "\u{251c}\u{2500}\u{2500} "; //      "|-- "
const INDENT: &str = "\u{2502}   "; //                    "|   "
const BRANCH_LAST: &str = "\u{2514}\u{2500}\u{2500} "; // "`-- "
const INDENT_LAST: &str = "    "; //                      "    "

impl<P: Provider + ProviderExt> Display for Navigate<P>
where
    <P as Provider>::Fragment: Display,
{
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        write!(f, "\x1b[H\x1b[J")?;

        let mut c = 0;
        let visible = self.view.visible();
        self.fmt_at(f, self.tree.root(), "".into(), &mut c, &visible)?;
        self.view.total.try_borrow_mut().unwrap().end = c;
        if c < visible.end {
            write!(f, "{}", "\n".repeat(visible.end - c))?;
        }

        self.tree
            .provider()
            .fmt_frag_path(f, self.tree.path_at(self.cursor))?;
        write!(f, "\r\n")?;

        if let State::Pending(v) = &self.state {
            for k in v {
                if k.is_ascii_graphic() {
                    write!(f, "{}", *k as char)
                } else {
                    write!(f, "<{k}>")
                }?;
            }
        }

        Ok(())
    }
}

impl<P: Provider> Navigate<P>
where
    <P as Provider>::Fragment: Display,
{
    fn fmt_at(
        &self,
        f: &mut Formatter,
        at: NodeRef,
        indent: String,
        current: &mut usize,
        visible: &Range<usize>,
    ) -> FmtResult {
        let node = self.tree.at(at);
        let frag = &node.fragment;

        if visible.contains(current) {
            if node.marked() {
                write!(f, " \x1b[4m")?;
            }
            if self.cursor == at {
                write!(f, "\x1b[7m")?;
            }
            write!(f, "{frag}\x1b[m")?;
        }

        if node.folded() {
            if visible.contains(current) {
                write!(f, "\r\n")?;
            }
            *current += 1;
            return Ok(());
        }
        let children = node.children().unwrap();
        if 0 == children.len() {
            if visible.contains(current) {
                write!(f, "\r\n")?;
            }
            *current += 1;
            return Ok(());
        }

        if 1 == children.len() {
            self.fmt_at(f, children[0], indent, current, visible)
        } else {
            if visible.contains(current) {
                write!(f, "\r\n")?;
            }
            *current += 1;

            let mut iter = children.iter();

            for it in iter.by_ref().take(children.len() - 1) {
                if visible.contains(current) {
                    write!(f, "{indent}{BRANCH}")?;
                }
                self.fmt_at(f, *it, format!("{indent}{INDENT}"), current, visible)?;
            }

            if visible.contains(current) {
                write!(f, "{indent}{BRANCH_LAST}")?;
            }
            self.fmt_at(
                f,
                *iter.next().unwrap(),
                format!("{indent}{INDENT_LAST}"),
                current,
                visible,
            )
        }
    }
}
