use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use mlua::{Function, Lua, LuaOptions, StdLib, Table};
use thiserror::Error;

mod input;
mod options;
mod scripting;
mod space;
mod view;

use crate::lua::structs::IndexPath;
use crate::navigate::input::Input;
use crate::navigate::options::Options;
use crate::navigate::space::Space;
use crate::navigate::view::ViewJumpBy;
use crate::provider::scratch::Scratch;
use crate::provider::Provider;
use crate::terminal::{self, RestoreWithPanicHook};

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
    spaces: Vec<Arc<Mutex<Space>>>,
    current_space: usize,

    user_script: Option<PathBuf>,

    input: Input,
    term: Option<RestoreWithPanicHook>,
    force_redraw: bool,

    exit: Option<String>,
    message: Message,

    options: Options,
    registers: BTreeMap<String, Vec<String>>,
}

impl Navigate {
    pub fn new(user_script: Option<PathBuf>) -> Self {
        Self {
            spaces: vec![Space::new(
                Box::new(Scratch::new("[Scratch]").unwrap()),
                "scratch".to_string(),
            )],
            current_space: 0,

            user_script,

            input: Input::default(),
            term: None,
            force_redraw: false,

            exit: None,
            message: Message::default(),

            options: Options::default(),
            registers: BTreeMap::default(),
        }
    }

    pub fn insert_space(&mut self, at: usize, provider: Box<dyn Provider>, provider_name: String) {
        if at <= self.current_space {
            self.current_space += 1;
        }
        self.spaces.insert(at, Space::new(provider, provider_name));
    }

    pub fn remove_space(&mut self, at: usize) {
        if 0 < self.current_space && at <= self.current_space {
            self.current_space -= 1;
        }
        self.spaces.remove(at); // space dropped now
        if self.spaces.is_empty() {
            self.spaces.push(Space::new(
                Box::new(Scratch::new("[Scratch]").unwrap()),
                "scratch".to_string(),
            ))
        }
    }

    pub fn replace_space(&mut self, at: usize, provider: Box<dyn Provider>, provider_name: String) {
        self.spaces[at] = Space::new(provider, provider_name);
    }

    pub fn swap_spaces(&mut self, at: usize, with: usize) {
        self.spaces.swap(at, with);
    }

    pub fn space(&self) -> impl Deref<Target = Space> + use<'_> {
        self.spaces[self.current_space].lock().unwrap()
    }

    pub fn space_mut(&mut self) -> impl DerefMut<Target = Space> + use<'_> {
        self.spaces[self.current_space].lock().unwrap()
    }

    /// Load the registers from `~/.cache/treest.hist`.
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

    /// Save the registers to `~/.cache/treest.hist`.
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

        // this could be delayed even more, but `self` in moved into lua
        // so it would need eg `treest:_lateinit`
        self.term = terminal::raw_with_panic_hook().ok();

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

    /// Set the message lines.
    ///
    /// This also resets the scrolling. Setting to an empty vector will essentially clear it.
    pub fn set_message_lines(&mut self, lines: Vec<String>) {
        self.message.lines = lines;
        self.message.scroll = 0;
    }

    /// Direct access to a register's history.
    ///
    /// `name` is trimmed of leading and trailing whitespaces.
    pub fn register_entries(&mut self, name: &str) -> &mut Vec<String> {
        self.registers.entry(name.trim().to_string()).or_default()
    }

    /// Push a new value
    ///
    /// Nothing happens if the old value was equal or the new one is empty.
    pub fn register_push(&mut self, name: &str, new: String) {
        if !new.is_empty() {
            let v = self.register_entries(name);
            if v.last().is_none_or(|old| *old != new) {
                v.push(new);
            }
        }
    }
}
