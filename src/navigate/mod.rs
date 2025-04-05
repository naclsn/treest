use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

use anyhow::Result;
use mlua::{Function, Lua, LuaOptions, StdLib, Table};
use thiserror::Error;

mod input;
mod options;
mod scripting;
mod view;

use crate::lua::structs::{IndexPath, NodeInfo, Target};
use crate::navigate::input::Input;
use crate::navigate::options::Options;
use crate::navigate::view::{View, ViewJumpBy};
use crate::providers::Provider;
use crate::terminal::{self, RestoreWithPanicHook};
use crate::tree::Node;

#[derive(Error, Debug)]
#[error("{0}")]
pub struct MainLoopExitText(String);

#[derive(Default)]
pub struct Message {
    lines: Vec<String>,
    scroll: usize,          // display offset
    previous_height: usize, // height from last render
}

pub struct Navigate {
    tree: Node,
    provider: Box<dyn Provider>,
    provider_name: String,
    view: View,
    cursor: (usize, IndexPath),

    user_script: Option<PathBuf>,

    input: Input,
    term: Option<RestoreWithPanicHook>,

    exit: Option<String>,
    message: Message,

    options: Options,
    registers: BTreeMap<String, Vec<String>>,
}

impl Navigate {
    pub fn new(
        user_script: Option<PathBuf>,
        provider: Box<dyn Provider>,
        provider_name: String,
    ) -> Self {
        Self {
            tree: Node::new(),
            provider,
            provider_name,
            view: View::default(),
            cursor: (0, IndexPath::default()),

            user_script,

            input: Input::default(),
            term: terminal::raw_with_panic_hook().ok(),

            exit: None,
            message: Message::default(),

            options: Options::default(),
            registers: BTreeMap::default(),
        }
    }

    fn load_registers(&mut self, f: &mut impl BufRead) -> Result<()> {
        while {
            let mut dashes = [0u8; 5];
            f.read_exact(&mut dashes)
                .is_ok_and(|_| *b"\n--- " == dashes)
        } {
            let mut len = vec![];
            f.read_until(b' ', &mut len)?;
            len.pop();
            let mut name = vec![0u8; String::from_utf8(len)?.parse()?];
            f.read_exact(&mut name)?;

            let hist = self.registers.entry(String::from_utf8(name)?).or_default();

            let mut count = vec![];
            f.read_until(b':', &mut count)?;
            count.pop();
            count.remove(0);
            for _ in 0..String::from_utf8(count)?.parse()? {
                let mut inde = [0u8; 3];
                f.read_exact(&mut inde)?;
                if *b"\n  " != inde {
                    break;
                }

                let mut len = vec![];
                f.read_until(b' ', &mut len)?;
                len.pop();
                let mut val = vec![0u8; String::from_utf8(len)?.parse()?];
                f.read_exact(&mut val)?;

                hist.push(String::from_utf8(val)?);
            }
        }
        Ok(())
    }

    fn save_registers(&self, f: &mut impl Write) -> Result<()> {
        writeln!(f)?;
        for (name, hist) in self.registers.iter().take(50) {
            writeln!(f, "--- {} {name} {}:", name.len(), hist.len())?;
            for val in hist.iter().rev().take(500).rev() {
                writeln!(f, "  {} {val}", val.len())?;
            }
        }
        Ok(())
    }

    pub fn main_loop(mut self) -> Result<(), MainLoopExitText> {
        let user_script = self.user_script.clone();

        if let Some(mut dir) = dirs::cache_dir() {
            dir.push("treest.hist");
            if let Ok(f) = File::open(&dir) {
                _ = self.load_registers(&mut BufReader::new(f));
            }
        }

        let lua = unsafe {
            Lua::unsafe_new_with(StdLib::ALL, LuaOptions::default().catch_rust_panics(false))
        };
        let g = lua.globals();
        g.raw_set("treest", self).unwrap();
        scripting::global_exports(&g, &lua).unwrap();
        let defaults: Table = lua
            .load_from_function(
                "defaults",
                lua.load(crate::include_etc!("defaults.lua"))
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

        terminal::cursor(false);

        let exit: String = lua
            .load(
                "repeat
    local action = treest:_tick()
    if action
      then
        ok, err = xpcall(action, debug.traceback)
        if not ok then treest:message(tostring(err)) end
    end
until treest.quitting
return treest:_atexit()
",
            )
            .set_name("=_heartbeat")
            .call(())
            .unwrap();

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

    /// Set the message lines.
    ///
    /// This also resets the scrolling. Setting to an empty vector will essentially clear it.
    pub fn set_message_lines(&mut self, lines: Vec<String>) {
        self.message.lines = lines;
        self.message.scroll = 0;
    }

    pub fn register_entries(&mut self, name: &str) -> &mut Vec<String> {
        self.registers.entry(name.trim().to_string()).or_default()
    }

    /// Push a new value, that is nothing happens if the old value was equal.
    pub fn register_push(&mut self, name: &str, new: String) {
        let v = self.register_entries(name);
        if v.last().is_none_or(|old| *old != new) {
            v.push(new);
        }
    }
}
