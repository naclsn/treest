use std::cell::RefCell;
use std::fmt::Display;
use std::mem;
use std::ops::Range;

mod display;
mod keymap;
mod options;

use options::Options;

use crate::reqres::ReqRes;
use crate::terminal;
use crate::tree::{NodeRef, Provider, ProviderExt, Tree};

pub struct Navigate<P: Provider> {
    tree: Tree<P>,
    cursor: NodeRef,
    pub state: State, // is updated by the driver loop (in main)
    pending: Vec<u8>,
    message: Option<String>,
    view: RefCell<View>, // is mutated during rendering to stay up to date
    options: Options,
}

pub enum State {
    Continue(ReqRes<(), u8>),
    Prompt(ReqRes<String, Option<Vec<String>>>),
}

impl Default for State {
    fn default() -> Self {
        Self::Continue(ReqRes::new(()))
    }
}

struct View {
    scroll: usize,
    total: Range<usize>,
    line_mapping: Vec<NodeRef>,
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

impl<P: Provider> Navigate<P> {
    pub fn new(provider: P) -> Self {
        let mut tree = Tree::new(provider);
        let cursor = tree.root();
        tree.unfold_at(cursor);
        Self {
            tree,
            cursor,
            state: State::default(),
            pending: Vec::new(),
            message: None,
            view: RefCell::new(View {
                scroll: 0,
                total: 0..0,
                line_mapping: Vec::new(),
            }),
            options: Options::default(),
        }
    }

    pub fn is_continue(&mut self) -> bool
    where
        P::Fragment: Display,
        P: ProviderExt,
    {
        match mem::take(&mut self.state) {
            State::Continue(r) => {
                self.pending.push(r.unwrap());
                match &self.pending[..] {
                    /* mouse */
                    [0x1b, b'[', b'M', button, _col, row] => match button {
                        /* left down */
                        32 => {
                            let which = &self.view.borrow().line_mapping;
                            let row = (*row - b'!') as usize;
                            if row < which.len() {
                                self.cursor = which[row];
                            }
                        }
                        /* right down */
                        34 => {
                            let hit = {
                                let which = &self.view.borrow().line_mapping;
                                let row = (*row - b'!') as usize;
                                if row < which.len() {
                                    self.cursor = which[row];
                                    true
                                } else {
                                    false
                                }
                            };
                            if hit {
                                if self.tree.at(self.cursor).folded() {
                                    self.unfold();
                                } else {
                                    self.fold();
                                }
                            }
                        }
                        /* up */ 35 => (),
                        /* wheel down */ 96 => self.view.borrow_mut().up(ViewJumpBy::Mouse),
                        /* wheel down */ 97 => self.view.borrow_mut().down(ViewJumpBy::Mouse),
                        _ => (),
                    },

                    /* ^B */ [0x02] => self.view.borrow_mut().up(ViewJumpBy::Win),
                    /* ^C */ [.., 0x03] => (),
                    /* ^D */ [0x04] => self.view.borrow_mut().down(ViewJumpBy::HalfWin),
                    /* ^E */ [0x05] => self.view.borrow_mut().down(ViewJumpBy::Line),
                    /* ^F */ [0x06] => self.view.borrow_mut().down(ViewJumpBy::Win),
                    /* ^G */ [.., 0x07] => (),
                    /* ^J */ [0x0a] => self.sibling_wrap(Direction::Next),
                    /* ^K */ [0x0b] => self.sibling_wrap(Direction::Prev),
                    /* ^U */ [0x15] => self.view.borrow_mut().up(ViewJumpBy::HalfWin),
                    /* ^Y */ [0x19] => self.view.borrow_mut().up(ViewJumpBy::Line),
                    //* ^[ */ [0x1b, ..] => todo!("wip"),
                    b"0" => self.root(),
                    b"H" => self.fold(),
                    b"L" => self.unfold(),
                    b"h" => self.leave(),
                    b"j" => self.sibling_sat(Direction::Next),
                    b"k" => self.sibling_sat(Direction::Prev),
                    b"l" => self.enter(),
                    b"q" => return false,
                    b" " => self.toggle_mark(),
                    b":" => {
                        self.state = State::Prompt(ReqRes::new(":".into()));
                        self.message = None;
                    }

                    _ => return true, // XXX(wip): for now skip `pending.clear()`
                }
            }

            State::Prompt(r) => {
                if let Some(mut r) = r.unwrap().take_if(|r| !r.is_empty()) {
                    for arg in r.iter_mut() {
                        if "%" == arg {
                            arg.clear();
                            let path = self.tree.path_at(self.cursor);
                            self.tree.provider().write_arg_path(arg, &path).unwrap();
                        }
                    }

                    match r[0].as_str() {
                        "se" | "set" => {
                            let r: Vec<_> = r[1..]
                                .iter()
                                .filter_map(|o| self.options.update(o))
                                .collect();
                            self.message = if r.is_empty() {
                                None
                            } else {
                                Some(r.join("  "))
                            };
                        }

                        "q" | "quit" => return false,

                        "ec" | "echo" => self.message = Some(r[1..].join(" ")),

                        _ => {
                            let info = self
                                .tree
                                .provider_command(&r)
                                .unwrap_or_else(|e| format!("\x1b[31m{e}\x1b[m"));
                            self.message = if info.is_empty() { None } else { Some(info) }
                        }
                    }
                }
            }
        }

        // if no early return: most common behavior
        self.pending.clear();
        true
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
