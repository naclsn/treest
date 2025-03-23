use std::collections::BTreeMap;
use std::ops::Range;
use std::path::PathBuf;

use anyhow::Result;
use mlua::{Function, Lua, LuaOptions, StdLib, Table};
use thiserror::Error;

pub mod display;
pub mod input;
pub mod options;
pub mod scripting;

use crate::lua::structs::{IndexPath, NodeInfo, Target};
use crate::navigate::input::Input;
use crate::navigate::options::Options;
use crate::providers::Provider;
use crate::terminal::{self, RestoreWithPanicHook};
use crate::tree::Node;

#[derive(Error, Debug)]
#[error("{0}")]
pub struct MainLoopExitText(String);

pub struct Message {
    lines: Vec<String>,
    offset: usize, // display offset
    interacted: bool, // true when the user is presumably done interacting/viewing
                   // which means that it will get cleared before the next action
}

pub struct Navigate {
    tree: Node,
    cursor: (usize, IndexPath),

    user_script: Option<PathBuf>,
    provider: Box<dyn Provider>,
    provider_name: String,

    input: Input,
    term: Option<RestoreWithPanicHook>,
    exit: Option<String>,

    message: Option<Message>,
    view: View,

    options: Options,
    registers: BTreeMap<String, Vec<String>>,
}

struct View {
    scroll: usize,
    total: Range<usize>,
    term_col: usize,
    term_row: usize,
    line_mapping: Vec<IndexPath>,
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
    pub fn new(
        user_script: Option<PathBuf>,
        provider: Box<dyn Provider>,
        provider_name: String,
    ) -> Self {
        Self {
            tree: Node::new(),
            cursor: (0, IndexPath::default()),

            user_script,
            provider,
            provider_name,

            input: Input::default(),
            term: terminal::raw_with_panic_hook().ok(),
            exit: None,

            message: None,
            view: View::default(),

            options: Options::default(),
            registers: BTreeMap::default(),
        }
    }

    pub fn main_loop(self) -> Result<(), MainLoopExitText> {
        let user_script = self.user_script.clone();

        let lua = unsafe {
            Lua::unsafe_new_with(StdLib::ALL, LuaOptions::default().catch_rust_panics(false))
        };
        let g = lua.globals();
        g.raw_set("treest", self).unwrap();
        scripting::global_exports(&g, &lua).unwrap();
        let defaults: Table = lua
            .load_from_function(
                "defaults",
                lua.load(include_str!("../defaults.lua"))
                    .set_name("@defaults.lua")
                    .into_function()
                    .unwrap(),
            )
            .unwrap();

        if let Some(path) = user_script {
            lua.load(path).exec().unwrap();
        } else {
            defaults
                .get::<Function>("init")
                .unwrap()
                .call::<()>(())
                .unwrap();
        }

        terminal::cursor_off();
        terminal::mouse_on();
        terminal::altscreen_on();

        let exit: String = lua
            .load(
                r#"repeat
    local action = treest:_tick()
    if action
      then
        ok, err = xpcall(action, debug.traceback)
        if not ok then treest:message(tostring(err)) end
    end
until treest.quitting
return treest:_atexit()
"#,
            )
            .set_name("=_heartbeat")
            .call(())
            .unwrap();

        terminal::cursor_on();
        terminal::mouse_off();
        terminal::altscreen_off();

        match exit {
            it if it.is_empty() => Ok(()),
            exit => Err(MainLoopExitText(exit)),
        }
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

    ///// The first (bool) argument to the closure is `true` when the path is "trusted".
    //fn map_at<R>(&self, at: &Target, f: impl FnOnce(bool, &[usize]) -> R) -> R {
    //    let (trust, path) = match &at {
    //        Target::Cursor => (true, &self.cursor.1[..self.cursor.0]),
    //        Target::Path(path) => (false, &path[..]),
    //        Target::TrustedPath(path) => (true, &path[..]),
    //    };
    //    f(trust, path)
    //}

    pub fn target_to_path(&self, at: Target) -> IndexPath {
        match at {
            Target::Cursor => self.cursor.1[..self.cursor.0].into(),
            Target::Path(path) => path, // XXX: untrusted path escalate to index path...
            Target::TrustedPath(path) => path,
        }
    }

    pub fn resolve_node(&self, at: &Target) -> Option<&Node> {
        match at {
            Target::Cursor => Some(self.tree.resolve_node(&self.cursor.1[..self.cursor.0])),
            Target::Path(path) => self.tree.try_resolve_node(path),
            Target::TrustedPath(path) => Some(self.tree.resolve_node(path)),
        }
    }

    pub fn resolve_node_mut(&mut self, at: &Target) -> Option<&mut Node> {
        match at {
            Target::Cursor => Some(self.tree.resolve_node_mut(&self.cursor.1[..self.cursor.0])),
            Target::Path(path) => self.tree.try_resolve_node_mut(path),
            Target::TrustedPath(path) => Some(self.tree.resolve_node_mut(path)),
        }
    }

    pub fn retrieve_node_info(&self, at: Target) -> Option<NodeInfo> {
        self.resolve_node(&at).map(|node| {
            let path = self.target_to_path(at);
            let node_path = self.tree.resolve(&path);
            NodeInfo {
                name: self.provider.display(&node_path[..].into()),
                components: self.provider.components(&node_path[..].into()),
                breadcrumbs: self.provider.breadcrumbs(&node_path[..].into()),
                child_count: node.is_loaded().then(|| node.child_count()),
                path,
            }
        })
    }

    /// Return the target node's child count if valid.
    pub fn set_folded(&mut self, at: Target, is: bool) -> Option<usize> {
        let node = self.resolve_node_mut(&at)?;
        if is || node.is_loaded() {
            node.set_folded(is);
            return Some(node.child_count());
        }
        Some(self.tree.load(
            &mut self.provider,
            // note: at this point the path can be trusted because resolve_node_mut above
            match &at {
                Target::Cursor => &self.cursor.1[..self.cursor.0],
                Target::Path(path) => &path[..],
                Target::TrustedPath(path) => &path[..],
            },
            // note: already know it isn't loaded, skip the check
            true,
            is,
        ))
    }
    pub fn get_folded(&self, at: Target) -> Option<bool> {
        self.resolve_node(&at).map(Node::is_folded)
    }

    pub fn set_marked(&mut self, at: Target, is: bool) {
        let Some(node) = self.resolve_node_mut(&at) else {
            return;
        };
        node.set_marked(is);
    }
    pub fn get_marked(&self, at: Target) -> Option<bool> {
        self.resolve_node(&at).map(Node::is_marked)
    }

    pub fn cursor_enter(&mut self) {
        let Some(child_count) = self.set_folded(Target::Cursor, false) else {
            return;
        };
        if 0 == child_count {
            return;
        }
        if self.cursor.1.len() == self.cursor.0 {
            self.cursor.1.push(0);
        }
        self.cursor.0 += 1;
    }

    pub fn cursor_leave(&mut self) {
        self.cursor.0 = self.cursor.0.saturating_sub(1);
    }

    pub fn cursor_next(&mut self, wrapping: bool) {
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

    pub fn cursor_prev(&mut self, wrapping: bool) {
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

    pub fn set_message_lines(&mut self, lines: Vec<String>) {
        if !lines.is_empty() {
            self.message = Some(Message {
                lines,
                offset: 0,
                interacted: false,
            });
        }
    }

    /// Push a new value, that is nothing happens if the old value was equal.
    pub fn register_push(&mut self, name: String, new: String) {
        let v = self.registers.entry(name).or_default();
        if v.last().is_none_or(|old| *old != new) {
            v.push(new);
        }
    }
}
