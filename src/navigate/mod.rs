use std::cell::RefCell;
use std::ops::Range;
use std::path::PathBuf;

mod display;
mod input;
mod scripting;

use crate::navigate::scripting::Scripting;
use crate::providers::Provider;
use crate::terminal;
use crate::tree::Node;

pub struct Navigate {
    tree: Node,
    provider: Box<dyn Provider>,
    cursor: Vec<usize>,

    input: input::Input,

    message: Option<String>,
    view: RefCell<View>, // is mutated during rendering to stay up to date

    scripting: scripting::Scripting,
}

struct View {
    scroll: usize,
    total: Range<usize>,
    line_mapping: Vec<Vec<usize>>, // XXX: eeee, for now yes and with an actual 'Cursor' type
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
}
