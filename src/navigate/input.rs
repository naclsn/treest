use std::fmt::Debug;
use std::fs::File;
use std::io::{self, Read};

use mlua::{Error as LuaError, FromLua, Function, IntoLua, Lua, Result as LuaResult, Value};

use crate::terminal;

pub struct Input {
    input: Box<dyn Iterator<Item = u8>>,
    recycle: Option<u8>,
    pending: Vec<u8>,
    pending_mouse_info: PendingMouseInfo,
    mappings: Vec<Mapping>,
    pending_reachable: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct PendingMouseInfo {
    pub col: u8,
    pub row: u8,
}
crate::impl_lua_conversion!(PendingMouseInfo { col, row });

struct Mapping(Vec<u8>, Function);

impl Debug for Mapping {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", terminal::keyseqstr(&self.0))
    }
}

impl Debug for Input {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Input")
            .field("recycle", &self.recycle.map(|b| terminal::keyseqstr(&[b])))
            .field("pending", &terminal::keyseqstr(&self.pending))
            .field("pending_mouse_info", &self.pending_mouse_info)
            .field("mappings", &self.mappings)
            .field("pending_reachable", &self.pending_reachable)
            .finish()
    }
}

impl Input {
    pub fn new() -> Self {
        Self {
            input: Box::new(
                match File::open("/dev/tty") {
                    Ok(f) => Box::new(f) as Box<dyn Read>,
                    Err(_) => Box::new(io::stdin()),
                }
                .bytes()
                .map_while(Result::ok),
            ),
            recycle: None,
            pending: Vec::new(),
            pending_mouse_info: PendingMouseInfo { row: 0, col: 0 },
            mappings: Vec::new(),
            pending_reachable: Vec::new(),
        }
    }

    pub fn tick(&mut self) -> Option<&Function> {
        let byte = self
            .recycle
            .take()
            .or_else(|| self.input.next())
            .expect("niy: eof stopping condition");
        // TODO: expose this so it can be called/mapped?
        if 3 == byte {
            self.pending.clear();
        }

        if self.pending.is_empty() {
            self.pending_reachable = self
                .mappings
                .iter()
                .enumerate()
                .filter_map(|(k, map)| if byte == map.0[0] { Some(k) } else { None })
                .collect();
            if self.pending_reachable.is_empty() {
                return None;
            }

            self.pending.push(byte);
        } else {
            match self.pending[..] {
                [.., 0x1b, b'[', b'M'] => {
                    self.pending.push(byte);
                    return None;
                }
                [.., 0x1b, b'[', b'M', _] => {
                    self.pending.push(b' ');
                    self.pending_mouse_info.col = byte.saturating_sub(b'!');
                    return None;
                }
                [.., 0x1b, b'[', b'M', _, _] => {
                    self.pending.push(b' ');
                    self.pending_mouse_info.row = byte.saturating_sub(b'!');
                }
                _ => self.pending.push(byte),
            }

            self.pending_reachable
                .retain(|index| self.mappings[*index].0.starts_with(&self.pending));
        }

        match self.pending_reachable[..] {
            [] => {
                // if we get here it means the latest byte made `retain` drop all potential
                // mappings; so excluding this byte, try to find an exact match
                if let Some(map) = self
                    .mappings
                    .iter()
                    .find(|map| map.0 == self.pending[..self.pending.len() - 1])
                {
                    let r = &map.1;
                    self.pending.clear();
                    self.recycle = Some(byte);
                    Some(r)
                } else {
                    self.pending.clear();
                    None
                }
            }

            [single] if self.mappings[single].0.len() == self.pending.len() => {
                self.pending.clear();
                Some(&self.mappings[single].1)
            }

            _ => None,
        }
    }

    pub fn get_pending(&self) -> &[u8] {
        &self.pending
    }

    pub fn get_pending_mouse_info(&self) -> PendingMouseInfo {
        self.pending_mouse_info.clone()
    }

    pub fn add_mapping(&mut self, sequence: Vec<u8>, action: Function) {
        if sequence.is_empty() {
            return;
        }
        if !self.pending.is_empty() && sequence.starts_with(&self.pending) {
            self.pending_reachable.push(self.mappings.len());
        }
        self.mappings.push(Mapping(sequence, action));
    }
}
