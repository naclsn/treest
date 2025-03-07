use std::fs::File;
use std::io::{self, Read};

use super::scripting::ScriptFnRef;

pub struct Input {
    input: Box<dyn Iterator<Item = u8>>,
    pending: Vec<u8>,
    pending_mouse_info: Option<PendingMouseInfo>,
    mappings: Vec<Mapping>,
    pending_reachable: Vec<usize>,
}

struct PendingMouseInfo {
    col: u8,
    row: u8,
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
            pending: Vec::new(),
            pending_mouse_info: None,
            mappings: Vec::new(),
            pending_reachable: Vec::new(),
        }
    }

    pub fn tick(&mut self) -> Option<ScriptFnRef> {
        let byte = self.input.next().expect("niy: eof stopping condition");
        if 3 == byte {
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
                let mut recycle = false;
                if let Some(map) = self
                    .mappings
                    .iter()
                    .find(|map| map.0 == self.pending[..self.pending.len() - 1])
                {
                    todo!("&map.1");
                    recycle = true;
                }

                self.pending.clear();
                self.pending_mouse_info = None;

                if recycle {
                    // fake-ly recusive: limited to the first branch
                    todo!("self.feed(byte)");
                }
                None
            }

            [single] if self.mappings[single].0.len() == self.pending.len() => {
                let action = self.mappings[single].1;

                self.pending.clear();
                self.pending_mouse_info = None;

                Some(action)
            }

            _ => None,
        }
    }

    //fn perform(&mut self, action: &ScriptFnRef) {
    //    //todo!("{action:?}");
    //    // likely that instead of performing the action here, it'll be returned to the navigate's
    //    // `feed` frame so it can have access to everything
    //    // well actually maybe not 'cause we could have 2 actions from a single byte fed..
    //}

    pub fn get_pending(&self) -> &[u8] {
        &self.pending
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
