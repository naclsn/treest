use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ops::Range;
use std::path::PathBuf;

use anyhow::Result;
use thiserror::Error;

mod display;
mod input;
mod options;
mod scripting;

use crate::navigate::input::Input;
use crate::navigate::options::Options;
use crate::navigate::scripting::Scripting;
use crate::providers::Provider;
use crate::terminal::{self, RestoreWithPanicHook};
use crate::tree::Node;

pub use scripting::make_engine;

#[derive(Error, Debug)]
#[error("{0}")]
pub struct MainLoopExitText(String);

type CursorLikePath = Vec<usize>;

pub struct Navigate {
    tree: Node,
    cursor: (usize, CursorLikePath),

    provider: Box<dyn Provider>,
    provider_name: String,

    input: Input,
    term: Option<RestoreWithPanicHook>,
    exit: Option<String>,

    message: Option<String>,
    view: RefCell<View>, // is mutated during rendering to stay up to date

    scripting: Scripting,
    options: Options,
    registers: BTreeMap<String, Vec<String>>,
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

impl Default for View {
    fn default() -> Self {
        Self {
            scroll: 0,
            total: 0..0,
            line_mapping: Vec::new(),
        }
    }
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
            Target::Cursor => &$self.cursor.1[..$self.cursor.0],
            Target::Path(path) => path,
        }
    };
}

impl Navigate {
    pub fn new(
        user_script: Option<PathBuf>,
        provider: Box<dyn Provider>,
        provider_name: String,
    ) -> Self {
        Self {
            tree: Node::new(),
            cursor: (0, vec![]),

            provider,
            provider_name,

            input: Input::new(),
            term: terminal::raw_with_panic_hook().ok(),
            exit: None,

            message: None,
            view: RefCell::default(),

            scripting: Scripting::new(user_script),
            options: Options::default(),
            registers: BTreeMap::new(),
        }
    }

    pub fn main_loop(self) -> Result<(), MainLoopExitText> {
        scripting::main_loop(self).map_err(MainLoopExitText)
    }

    pub fn is_cursor_root(&self) -> bool {
        0 == self.cursor.0
    }

    pub fn cursor(&self) -> &[usize] {
        &self.cursor.1[..self.cursor.0]
    }

    pub fn cursor_head(&self) -> &[usize] {
        &self.cursor.1[..self.cursor.0 - 1]
    }

    pub fn cursor_tail_mut(&mut self) -> &mut usize {
        &mut self.cursor.1[self.cursor.0 - 1]
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
        if self.cursor.1.len() == self.cursor.0 {
            self.cursor.1.push(0);
        }
        self.cursor.0 += 1;
    }

    pub fn leave(&mut self) {
        self.cursor.0 = self.cursor.0.saturating_sub(1);
    }

    pub fn next(&mut self, wrapping: bool) {
        if 0 == self.cursor.0 {
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
        self.cursor.1.truncate(self.cursor.0);
    }

    pub fn prev(&mut self, wrapping: bool) {
        if 0 == self.cursor.0 {
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
        self.cursor.1.truncate(self.cursor.0);
    }

    /// Push a new value, that is nothing happens if the old value was equal.
    pub fn register_push(&mut self, name: String, new: String) {
        let v = self.registers.entry(name).or_default();
        if v.last().is_none_or(|old| *old != new) {
            v.push(new);
        }
    }
}
