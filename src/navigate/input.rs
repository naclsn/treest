use std::collections::VecDeque;
use std::fmt::{Debug, Formatter, Result as FmtResult};
use std::io::{self, Bytes, Read, Result as IoResult, Stdin};
use std::iter::MapWhile;

use crate::lua::structs::{MappingFn, PendingMouseInfo, PromptAnsCallback};
use crate::prompt::{Prompt, PromptState};
use crate::terminal;

type IoResultU8Ok = fn(IoResult<u8>) -> Option<u8>;
pub struct Input {
    input: MapWhile<Bytes<Stdin>, IoResultU8Ok>,
    recycle: VecDeque<u8>,

    pending: Vec<u8>,
    pending_mouse_info: PendingMouseInfo,
    mappings: Vec<Mapping>,
    pending_reachable: Vec<usize>,

    prompt: Option<(Prompt, Option<PromptAnsCallback>)>,
}

struct Mapping(Vec<u8>, MappingFn);

impl Debug for Mapping {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", terminal::keyseqstr(&self.0))
    }
}

impl Debug for Input {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("Input")
            .field(
                "recycle",
                &terminal::keyseqstr(&self.recycle.iter().copied().collect::<Vec<_>>()),
            )
            .field("pending", &terminal::keyseqstr(&self.pending))
            .field("pending_mouse_info", &self.pending_mouse_info)
            .field("mappings", &self.mappings)
            .field("pending_reachable", &self.pending_reachable)
            .finish()
    }
}

impl Default for Input {
    fn default() -> Self {
        Self {
            input: io::stdin().bytes().map_while(Result::ok),
            recycle: VecDeque::new(),

            pending: Vec::default(),
            pending_mouse_info: PendingMouseInfo { col: 0, row: 0 },
            mappings: Vec::default(),
            pending_reachable: Vec::default(),

            prompt: None,
        }
    }
}

pub enum InputTickResponse {
    Noop,
    EndOfInput,
    CallbackMapping(MappingFn),
    CallbackPrompt {
        ps: String,
        then: PromptAnsCallback,
        ans: String,
    },
}

impl Input {
    pub fn tick(&mut self) -> InputTickResponse {
        use InputTickResponse::*;
        let Some(byte) = self.recycle.pop_front().or_else(|| self.input.next()) else {
            return EndOfInput;
        };

        if let Some((prompt, then)) = self.prompt.take() {
            return match prompt.feed(byte) {
                PromptState::Again { up, prompt } => {
                    eprint!("{up}");
                    self.prompt = Some((prompt, then));
                    Noop
                }
                PromptState::Final { ps, ans } => {
                    terminal::cursor(false); // set from back in Navitate::prompt
                    eprint!("\r\x1b[K");
                    if let Some((then, ans)) = Option::zip(then, ans) {
                        CallbackPrompt { ps, then, ans }
                    } else {
                        Noop
                    }
                }
            };
        }

        if self.pending.is_empty() {
            self.pending_reachable = self
                .mappings
                .iter()
                .enumerate()
                .filter_map(|(k, map)| if byte == map.0[0] { Some(k) } else { None })
                .collect();
            if self.pending_reachable.is_empty() {
                return Noop;
            }

            self.pending.push(byte);
        } else {
            match self.pending[..] {
                [.., 0x1b, b'[', b'M'] => {
                    self.pending.push(byte);
                    return Noop;
                }
                [.., 0x1b, b'[', b'M', _] => {
                    self.pending.push(b' ');
                    self.pending_mouse_info.col = byte.saturating_sub(b'!');
                    return Noop;
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
                    self.recycle.push_back(byte);
                    CallbackMapping(r.clone())
                } else {
                    self.pending.clear();
                    Noop
                }
            }

            [single] if self.mappings[single].0.len() == self.pending.len() => {
                self.pending.clear();
                CallbackMapping(self.mappings[single].1.clone())
            }

            _ => Noop,
        }
    }

    pub fn recycle(&mut self, seq: impl IntoIterator<Item = u8>) {
        self.recycle.extend(seq);
    }

    pub fn has_recycle(&self) -> bool {
        !self.recycle.is_empty()
    }

    pub fn get_pending(&self) -> &[u8] {
        &self.pending
    }

    pub fn get_pending_mouse_info(&self) -> PendingMouseInfo {
        self.pending_mouse_info.clone()
    }

    pub fn get_prompt(&self) -> Option<&Prompt> {
        self.prompt.as_ref().map(|(prompt, _)| prompt)
    }

    pub fn set_prompt(&mut self, prompt: Prompt, then: Option<PromptAnsCallback>) {
        self.prompt = Some((prompt, then));
    }

    pub fn add_mapping(&mut self, sequence: Vec<u8>, action: MappingFn) {
        if sequence.is_empty() {
            return;
        }
        if !self.pending.is_empty() && sequence.starts_with(&self.pending) {
            self.pending_reachable.push(self.mappings.len());
        }
        self.mappings.push(Mapping(sequence, action));
    }

    pub fn get_mapping(&self, sequence: Vec<u8>) -> Option<&MappingFn> {
        self.mappings
            .iter()
            .find(|map| sequence == map.0)
            .map(|map| &map.1)
    }

    pub fn pop_mapping(&mut self, sequence: Vec<u8>) -> Option<MappingFn> {
        let k = self.mappings.iter().position(|map| sequence == map.0)?;
        if let Some(a) = self
            .pending_reachable
            .iter_mut()
            .find(|a| self.mappings.len() - 1 == **a)
        {
            *a = k;
        }
        Some(self.mappings.swap_remove(k).1)
    }
}
