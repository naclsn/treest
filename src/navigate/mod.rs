use std::cell::RefCell;
use std::ops::Range;
use std::path::PathBuf;

mod display;
mod input;
mod scripting;

use crate::navigate::scripting::Scripting;
use crate::providers::Provider;
use crate::terminal;
use crate::tree::{Cursor, Node};

pub struct Navigate {
    tree: Node,
    provider: Box<dyn Provider>,
    cursor: Cursor,

    input: input::Input,

    message: Option<String>,
    view: RefCell<View>, // is mutated during rendering to stay up to date

    scripting: scripting::Scripting,
}

struct View {
    scroll: usize,
    total: Range<usize>,
    line_mapping: Vec<Cursor>,
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
        let end = self.total.end;
        if self.scroll + by < end {
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

impl Navigate {
    pub fn new(user_script: Option<PathBuf>, provider: Box<dyn Provider>) -> Self {
        Self {
            tree: Node::new(),
            provider,
            cursor: Vec::new(),
            input: input::Input::new(),
            message: None,
            view: RefCell::new(View {
                scroll: 0,
                total: 0..0,
                line_mapping: Vec::new(),
            }),
            scripting: Scripting::new(user_script),
        }
    }

    pub fn main_loop(self) {
        scripting::main_loop(self);
    }

    pub fn unfold(&mut self, path: &[usize]) {
        self.tree.unfold(&mut self.provider, path);
    }

    pub fn unfold_cursor(&mut self) {
        self.tree.unfold(&mut self.provider, &self.cursor);
    }

    pub fn enter(&mut self) {
        // TODO: !!
        self.cursor.push(0);
    }

    pub fn leave(&mut self) {
        // TODO: !!
        self.cursor.pop();
    }

    pub fn next(&mut self, wrapping: bool) {
        let l = self.cursor.len();
        if 0 < l {
            let m = self.tree.resolve_node(&self.cursor[..l - 1]).child_count() - 1;
            self.cursor.last_mut().map(|k| match wrapping {
                false if *k < m => *k += 1,
                true if *k == m => *k = 0,
                true => *k += 1,
                _ => (),
            });
        }
    }

    pub fn prev(&mut self, wrapping: bool) {
        let l = self.cursor.len();
        if 0 < l {
            let m = self.tree.resolve_node(&self.cursor[..l - 1]).child_count() - 1;
            self.cursor.last_mut().map(|k| match wrapping {
                false if 0 < *k => *k -= 1,
                true if 0 == *k => *k = m,
                true => *k -= 1,
                _ => (),
            });
        }
    }
}
