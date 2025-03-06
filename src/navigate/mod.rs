use std::cell::RefCell;
use std::fmt::Display;
use std::io::Result as IoResult;
use std::ops::Range;
use std::process::{Command as ProcCommand, ExitStatus as ProcStatus, Output as ProcOutput};

use rhai::Engine;

mod api;
mod display;
mod input;

use crate::terminal;
use crate::tree::Node;
use crate::providers::Provider;

pub struct Navigate {
    tree: Node,
    provider: Box<dyn Provider>,
    cursor: Vec<usize>,

    input: input::Input,

    message: Option<String>,
    view: RefCell<View>, // is mutated during rendering to stay up to date

    // XXX: is that just, like, RefCell or omsethingelse?
    engine: Option<Engine>, // `.take`n during evaluation
    // also think it could be just Engine directly
}

struct View {
    scroll: usize,
    total: Range<usize>,
    line_mapping: Vec<usize>,
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
    pub fn new(mut provider: impl Provider + 'static) -> Self {
        let mut tree = Node::new();
        tree.unfold(&mut provider, &[]);
        Self {
            tree,
            provider: Box::new(provider),
            cursor: Vec::new(),
            input: {
                let mut r = input::Input::default();
                r.add_mapping(b"ab".to_vec(), ());
                r.add_mapping(b"abc".to_vec(), ());
                r
            },
            message: None,
            view: RefCell::new(View {
                scroll: 0,
                total: 0..0,
                line_mapping: Vec::new(),
            }),
            engine: Some(api::init_engine()),
        }
    }

    pub fn resolve_cursor(&self) -> &Node {
        //self.tree.resolve();
        self.cursor.iter().fold(&self.tree, |acc, cur| &acc.children().unwrap()[*cur])
    }

    pub fn feed(&mut self, byte: u8) {
        self.input.feed(byte);
        if 3 == byte {
            panic!();
        }

        api::run_callback(self, "hello");
    }

    /*
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

    pub fn toggle_fold(&mut self) {
        if self.tree.at(self.cursor).folded() {
            self.unfold();
        } else {
            self.fold();
        }
    }

    pub fn toggle_mark(&mut self) {
        self.tree.toggle_mark_at(self.cursor);
    }

    // "%"
    pub fn curr_path_string(&self) -> String {
        let mut r = String::new();
        self.tree
            .provider()
            .write_arg_path(&mut r, &self.tree.path_at(self.cursor))
            .unwrap();
        r
    }
    */
}
