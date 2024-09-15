use std::fmt::{Display, Formatter, Result as FmtResult};
use std::iter;

use crate::tree::{NodeRef, Provider, Tree};

pub struct Navigate<P: Provider> {
    tree: Tree<P>,
    cursor: NodeRef,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Direction {
    Previous,
    Next,
}

impl Direction {
    pub fn go(&self, k: usize) -> usize {
        match self {
            Direction::Previous => k - 1,
            Direction::Next => k + 1,
        }
    }

    pub fn go_sat(&self, k: usize, m: usize) -> usize {
        match self {
            Direction::Previous if 0 != k => k - 1,
            Direction::Next if k < m - 1 => k + 1,
            _ => k,
        }
    }

    pub fn go_wrap(&self, k: usize, m: usize) -> usize {
        match self {
            Direction::Previous if 0 == k => m - 1,
            Direction::Next if k == m - 1 => 0,
            _ => self.go(k),
        }
    }
}

impl<P: Provider> Navigate<P> {
    pub fn new(provider: P) -> Self {
        let tree = Tree::new(provider);
        let cursor = tree.root();
        Self { tree, cursor }
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

impl<P: Provider> Display for Navigate<P>
where
    <P as Provider>::Fragment: Display,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        let mut stack = vec![(0, self.tree.root())];

        while let Some((depth, at)) = stack.pop() {
            let node = self.tree.at(at);
            let frag = &node.fragment;
            write!(
                f,
                "{:1$}{2}{frag}\x1b[m",
                "",
                depth * 4,
                if self.cursor == at { "\x1b[7m" } else { "" }
            )?;

            if !node.folded() {
                // TODO: sort and filter
                let mut children = node.children();

                if 1 == children.len() {
                    stack.push((depth, children[0]));
                } else {
                    writeln!(f)?;
                    children.reverse();
                    stack.extend(iter::repeat(depth + 1).zip(children));
                }
            } else {
                writeln!(f)?;
            }

            if 999 < stack.len() {
                break;
            }
        }

        Ok(())
    }
}
