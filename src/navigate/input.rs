use std::fmt::Debug;
use std::fs::File;
use std::io::{self, Read};

use crate::navigate::scripting::ScriptFnRef;
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

struct Mapping(Vec<u8>, ScriptFnRef);

impl Debug for Input {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        #[allow(dead_code)]
        #[derive(Debug)]
        struct Input<'a> {
            recycle: Option<String>,
            pending: String,
            pending_mouse_info: &'a PendingMouseInfo,
            mappings: Vec<String>,
            pending_reachable: &'a Vec<usize>,
        }
        Input {
            recycle: self.recycle.map(|b| terminal::keyseqstr(&[b])),
            pending: terminal::keyseqstr(&self.pending),
            pending_mouse_info: &self.pending_mouse_info,
            mappings: self
                .mappings
                .iter()
                .map(|m| terminal::keyseqstr(&m.0))
                .collect(),
            pending_reachable: &self.pending_reachable,
        }
        .fmt(f)
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

    pub fn tick(&mut self) -> Option<ScriptFnRef> {
        let byte = self
            .recycle
            .take()
            .or_else(|| self.input.next())
            .expect("niy: eof stopping condition");
        if 3 == byte {
            if self.pending.is_empty() {
                panic!("<C-C>");
            }
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
                    let r = map.1;
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
                Some(self.mappings[single].1)
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

    pub fn add_mapping(&mut self, sequence: Vec<u8>, action: ScriptFnRef) {
        if sequence.is_empty() {
            return;
        }
        if !self.pending.is_empty() && sequence.starts_with(&self.pending) {
            self.pending_reachable.push(self.mappings.len());
        }
        self.mappings.push(Mapping(sequence, action));
    }
}
