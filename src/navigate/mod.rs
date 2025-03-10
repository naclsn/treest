use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;

mod display;
mod input;
mod options;
mod scripting;

use crate::navigate::input::Input;
use crate::navigate::options::Options;
use crate::navigate::scripting::Scripting;
use crate::providers::Provider;
use crate::terminal;
use crate::tree::Node;

type CursorLikePath = Vec<usize>;

pub struct Navigate {
    tree: Node,
    provider: Box<dyn Provider>,
    cursor: (CursorLikePath, usize),

    input: input::Input,

    prompt_history: HashMap<String, Vec<String>>,

    message: Option<String>,
    view: RefCell<View>, // is mutated during rendering to stay up to date

    scripting: Scripting,
    options: Options,
}

#[derive(Debug, Clone, Copy)]
pub enum Target<'a> {
    Cursor,
    Path(&'a [usize]),
}

struct View {
    scroll: usize,
    total: Range<usize>,
    line_mapping: Vec<CursorLikePath>,
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

macro_rules! as_path {
    ($self:ident, $at:expr) => {
        match $at {
            Target::Cursor => &$self.cursor.0[..$self.cursor.1],
            Target::Path(path) => path,
        }
    };
}

impl Navigate {
    pub fn new(user_script: Option<PathBuf>, provider: Box<dyn Provider>) -> Self {
        Self {
            tree: Node::new(),
            provider,
            cursor: (vec![], 0),

            input: Input::new(),

            prompt_history: HashMap::new(),

            message: None,
            view: RefCell::new(View {
                scroll: 0,
                total: 0..0,
                line_mapping: Vec::new(),
            }),

            scripting: Scripting::new(user_script),
            options: Options::default(),
        }
    }

    pub fn main_loop(self) {
        scripting::main_loop(self);
    }

    pub fn cursor(&self) -> &[usize] {
        &self.cursor.0[..self.cursor.1]
    }

    pub fn cursor_head(&self) -> &[usize] {
        &self.cursor.0[..self.cursor.1 - 1]
    }

    pub fn cursor_tail_mut(&mut self) -> &mut usize {
        &mut self.cursor.0[self.cursor.1 - 1]
    }

    pub fn set_folded(&mut self, at: Target, is: bool) -> usize {
        self.tree
            .load(&mut self.provider, as_path!(self, at), false, is)
    }
    pub fn get_folded(&self, at: Target) -> bool {
        self.tree.resolve_node(as_path!(self, at)).is_folded()
    }

    pub fn set_marked(&mut self, at: Target, is: bool) {
        self.tree
            .resolve_node_mut(as_path!(self, at))
            .set_marked(is);
    }
    pub fn get_marked(&self, at: Target) -> bool {
        self.tree.resolve_node(as_path!(self, at)).is_marked()
    }

    pub fn enter(&mut self) {
        let child_count = self.set_folded(Target::Cursor, false);
        if 0 == child_count {
            return;
        }
        if self.cursor.0.len() == self.cursor.1 {
            self.cursor.0.push(0);
        }
        self.cursor.1 += 1;
    }

    pub fn leave(&mut self) {
        self.cursor.1 = self.cursor.1.saturating_sub(1);
    }

    pub fn next(&mut self, wrapping: bool) {
        if 0 == self.cursor.1 {
            return;
        }
        let m = self.tree.resolve_node(self.cursor_head()).child_count();
        if 0 == m {
            return;
        }
        let k = self.cursor_tail_mut();
        match wrapping {
            false if *k < m - 1 => *k += 1,
            true if *k == m - 1 => *k = 0,
            true => *k += 1,
            _ => (),
        }
        self.cursor.0.truncate(self.cursor.1);
    }

    pub fn prev(&mut self, wrapping: bool) {
        if 0 == self.cursor.1 {
            return;
        }
        let m = self.tree.resolve_node(self.cursor_head()).child_count();
        if 0 == m {
            return;
        }
        let k = self.cursor_tail_mut();
        match wrapping {
            false if 0 < *k => *k -= 1,
            true if 0 == *k => *k = m - 1,
            true => *k -= 1,
            _ => (),
        }
        self.cursor.0.truncate(self.cursor.1);
    }
}
