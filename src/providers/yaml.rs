use std::fmt::{Display, Formatter, Result as FmtResult};
use std::fs::File;
use std::io;
use std::marker::PhantomPinned;
use std::pin::Pin;

use serde_yml::{self, Value};

use super::{
    generic::{Generic, GenericValue},
    Error,
};

pub struct Yaml(Pin<Box<(Value, PhantomPinned)>>);

#[derive(Default, PartialEq, PartialOrd)]
pub enum Index {
    #[default]
    Root,
    Index(usize),
    KeyBool(bool),
    KeyNumber(f64),
    KeyString(String),
}

impl Display for Index {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Index::Root => write!(f, " "),
            Index::Index(i) => write!(f, "[{i}]"),
            Index::KeyBool(b) => write!(f, ".{b}"),
            Index::KeyNumber(n) => write!(f, "[{n}]"),
            Index::KeyString(s) if s.chars().any(|c| ' ' == c || '.' == c) => write!(f, ".'{s}'"),
            Index::KeyString(s) => write!(f, ".{s}"),
        }
    }
}

impl GenericValue for Value {
    type Index = Index;

    fn children(&self) -> Vec<(Self::Index, &Self)> {
        use Value::*;
        match self {
            Null | Bool(_) | Number(_) | String(_) => Vec::new(),

            Sequence(v) => v
                .iter()
                .enumerate()
                .map(|(k, v)| (Index::Index(k), v))
                .collect(),
            Mapping(m) => m
                .iter()
                .map(|(k, v)| {
                    (
                        match k {
                            Bool(b) => Index::KeyBool(*b),
                            Number(n) => Index::KeyNumber(n.as_f64().unwrap()),
                            String(s) => Index::KeyString(s.clone()),
                            Null | Sequence(_) | Mapping(_) | Tagged(_) => todo!(),
                        },
                        v,
                    )
                })
                .collect(),
            Tagged(t) => t.value.children(),
        }
    }

    fn fmt_leaf(&self, f: &mut Formatter<'_>) -> FmtResult {
        use Value::*;
        match self {
            Null => write!(f, "\x1b[35mnull"),
            Bool(b) => write!(f, "\x1b[35m{b}"),
            Number(n) => write!(f, "\x1b[33m{n}"),
            String(s) => write!(f, "\x1b[32m\"{s}\""),
            Sequence(s) => write!(f, "({}) ", s.len()),
            Mapping(m) => write!(f, "({}) ", m.len()),
            Tagged(t) => write!(f, "!{} ", t.tag.string),
        }
    }
}

impl Generic for Yaml {
    type Value = Value;

    fn root(&self) -> &Pin<Box<(Self::Value, PhantomPinned)>> {
        &self.0
    }
}

impl Yaml {
    pub fn new(path: &str) -> Result<Self, Error> {
        if path.is_empty() {
            serde_yml::from_reader(io::stdin())
        } else {
            serde_yml::from_reader(File::open(path).map_err(Error::IoErr)?)
        }
        .map(|value| Self(Box::pin((value, PhantomPinned))))
        .map_err(|_| todo!())
    }
}
