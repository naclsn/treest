use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ops::Range;
use std::path::PathBuf;

use anyhow::Result;
use mlua::{FromLua, Function, Lua, Result as LuaResult, Table, Value};
use thiserror::Error;

pub mod display;
pub mod input;
pub mod options;
pub mod scripting;

use crate::lua::typedoc::{LuaTypeAliasDoc, LuaTypeDoc};
use crate::navigate::input::Input;
use crate::navigate::options::Options;
use crate::providers::Provider;
use crate::terminal::{self, RestoreWithPanicHook};
use crate::tree::Node;

#[derive(Error, Debug)]
#[error("{0}")]
pub struct MainLoopExitText(String);

type CursorLikePath = Vec<usize>;

pub struct Navigate {
    tree: Node,
    cursor: (usize, CursorLikePath),

    user_script: Option<PathBuf>,
    provider: Box<dyn Provider>,
    provider_name: String,

    input: Input,
    term: Option<RestoreWithPanicHook>,
    exit: Option<String>,

    message: Option<String>,
    view: RefCell<View>, // is mutated during rendering to stay up to date TODO: don't use Display

    options: Options,
    registers: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone)]
pub enum Target {
    Cursor,
    Path(Vec<usize>),
    TrustedPath(Vec<usize>),
    //RelativePath(Vec<usize>),
}

impl FromLua for Target {
    fn from_lua(value: Value, lua: &Lua) -> LuaResult<Self> {
        match value {
            Value::Nil => Ok(Target::Cursor),
            // TODO: RelativePath, maybe with negative number as first item
            _ => Ok(Target::Path(Vec::from_lua(value, lua)?)),
        }
    }
}
impl LuaTypeDoc for Target {
    fn lua_type_doc() -> String {
        "Target".to_string()
    }
}
impl LuaTypeAliasDoc for Target {
    fn lua_type_doc_alias_to() -> String {
        "integer[]?".to_string()
    }
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

impl Navigate {
    pub fn new(
        user_script: Option<PathBuf>,
        provider: Box<dyn Provider>,
        provider_name: String,
    ) -> Self {
        Self {
            tree: Node::new(),
            cursor: (0, vec![]),

            user_script,
            provider,
            provider_name,

            input: Input::new(),
            term: terminal::raw_with_panic_hook().ok(),
            exit: None,

            message: None,
            view: RefCell::default(),

            options: Options::default(),
            registers: BTreeMap::new(),
        }
    }

    pub fn main_loop(self) -> Result<(), MainLoopExitText> {
        let user_script = self.user_script.clone();

        let lua = unsafe { Lua::unsafe_new() };
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

    /// Return the target node's child count if valid.
    pub fn set_folded(&mut self, at: Target, is: bool) -> Option<usize> {
        let Some(node) = self.resolve_node_mut(&at) else {
            return None;
        };
        if !is || node.is_loaded() {
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

    /// Push a new value, that is nothing happens if the old value was equal.
    pub fn register_push(&mut self, name: String, new: String) {
        let v = self.registers.entry(name).or_default();
        if v.last().is_none_or(|old| *old != new) {
            v.push(new);
        }
    }
}
