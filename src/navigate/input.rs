use std::fs::File;
use std::io::{self, Read};

use crate::navigate::scripting::ScriptFnRef;

pub struct Input {
    input: Box<dyn Iterator<Item = u8>>,
    recycle: Option<u8>,
    pending: Vec<u8>,
    pending_mouse_info: Option<PendingMouseInfo>,
    mappings: Vec<Mapping>,
    pending_reachable: Vec<usize>,
}

#[derive(Clone)]
pub struct PendingMouseInfo {
    pub col: u8,
    pub row: u8,
}

struct Mapping(Vec<u8>, ScriptFnRef);

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
            pending_mouse_info: None,
            mappings: Vec::new(),
            pending_reachable: Vec::new(),
        }
    }

    fn clear_pending(&mut self) {
        self.pending.clear();
        self.pending_mouse_info = None;
    }

    pub fn tick(&mut self) -> Option<ScriptFnRef> {
        let byte = self
            .recycle
            .take()
            .or_else(|| self.input.next())
            .expect("niy: eof stopping condition");
        if 3 == byte {
            self.clear_pending();
            panic!("<C-C>");
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
            self.pending.push(byte);
            if let [.., 0x1b, b'[', b'M', _, col, row] = &self.pending[..] {
                self.pending_mouse_info = Some(PendingMouseInfo {
                    col: *col - b'!',
                    row: *row - b'!',
                });
                let l = self.pending.len();
                self.pending[l - 2] = b' ';
                self.pending[l - 1] = b' ';
            }

            self.pending_reachable
                .retain(|index| self.mappings[*index].0.starts_with(&self.pending));
        }

        match self.pending_reachable[..] {
            [] => {
                // if we get here it means the latest byte made `retain` drop all potential
                // mapping; so excluding this byte, try to find an exact match
                if let Some(map) = self
                    .mappings
                    .iter()
                    .find(|map| map.0 == self.pending[..self.pending.len() - 1])
                {
                    let r = map.1;
                    self.clear_pending();
                    self.recycle = Some(byte);
                    return Some(r);
                }

                self.clear_pending();
                None
            }

            [single] if self.mappings[single].0.len() == self.pending.len() => {
                self.clear_pending();
                Some(self.mappings[single].1)
            }

            _ => None,
        }
    }

    pub fn get_pending(&self) -> &[u8] {
        &self.pending
    }

    pub fn get_pending_mouse_info(&self) -> Option<&PendingMouseInfo> {
        self.pending_mouse_info.as_ref()
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
