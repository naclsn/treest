use std::fmt::{Display, Formatter, Result as FmtResult};
use std::iter;

use crate::tree::{NodeRef, Provider, Tree};

pub struct Navigate<P: Provider> {
    tree: Tree<P>,
    cursor: NodeRef,
    state: State,
}

pub enum State {
    Continue,
    Quit,
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
        }
    }

    pub fn feed(&mut self, key: u8) -> &State {
        match self.state {
            State::Continue => match key {
                b'0' => self.root(),
                b'H' => self.fold(),
                b'L' => self.unfold(),
                b'h' => self.leave(),
                b'j' => self.sibling_sat(Direction::Next),
                b'k' => self.sibling_sat(Direction::Prev),
                0x09 => self.sibling_wrap(Direction::Prev),
                0x0a => self.sibling_wrap(Direction::Next),
                b'l' => self.enter(),
                b'q' => self.state = State::Quit,
                _ => (),
            },
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
        let siblings = self.tree.at(self.tree.at(self.cursor).parent()).children();
        let me = siblings.iter().position(|c| self.cursor == *c).unwrap();
        self.cursor = siblings[dir.go_sat(me, siblings.len())];
    }

    pub fn sibling_wrap(&mut self, dir: Direction) {
        let siblings = self.tree.at(self.tree.at(self.cursor).parent()).children();
        let me = siblings.iter().position(|c| self.cursor == *c).unwrap();
        self.cursor = siblings[dir.go_wrap(me, siblings.len())];
    }

    pub fn enter(&mut self) {
        self.unfold();
        if let Some(child) = self.tree.at(self.cursor).first_child() {
            self.cursor = child;
        }
    }

    pub fn leave(&mut self) {
        self.cursor = self.tree.at(self.cursor).parent();
    }
}

// TODO: pretty
//const INDENT: &str = "\u{2502}  "; //                    "|  "
//const BRANCH: &str = "\u{251c}\u{2500}\u{2500}"; //      "|--"
//const BRANCH_LAST: &str = "\u{2514}\u{2500}\u{2500}"; // "`--"
//const INDENT_WIDTH: u16 = 4;

impl<P: Provider> Display for Navigate<P>
where
    <P as Provider>::Fragment: Display,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "\x1b[H\x1b[J")?;

        let mut stack = vec![(0, self.tree.root())];
        let mut single = true;

        while let Some((depth, at)) = stack.pop() {
            if single {
                single = false
            } else {
                write!(f, "{:1$}", "", depth * 4)?;
            }
            if self.cursor == at {
                write!(f, "\x1b[7m")?;
            }

            let node = self.tree.at(at);
            let frag = &node.fragment;
            write!(f, "{frag}\x1b[m")?;

            if !node.folded() {
                // TODO: sort and filter (not here)
                let mut children = node.children();

                if 1 == children.len() {
                    single = true;
                    stack.push((depth, children[0]));
                } else {
                    write!(f, "\n\r")?;
                    children.reverse();
                    stack.extend(iter::repeat(depth + 1).zip(children));
                }
            } else {
                write!(f, "\n\r")?;
            }

            if 100 < stack.len() {
                break;
            }
        }

        Ok(())
    }
}
