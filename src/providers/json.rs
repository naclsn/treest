use std::fmt::{Display, Formatter, Result as FmtResult};
use std::fs::File;
use std::io;
use std::marker::PhantomPinned;
use std::pin::Pin;

use serde_json::{self, Value};

use super::{
    generic::{Generic, GenericValue},
    Error,
};

pub struct Json(Pin<Box<(Value, PhantomPinned)>>);

#[derive(Default, PartialEq, PartialOrd)]
pub enum Index {
    #[default]
    Root,
    Num(usize),
    Key(String),
}

impl Display for Index {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        use Index::*;

        match self {
            Root => write!(f, "$"),
            Num(index) => write!(f, "[{index}]"),
            // TODO: provider-specific setting to show all keys as in JSON (ie. `"key"`)
            Key(key)
                if (key.as_bytes()[0].is_ascii_alphabetic() || b'_' == key.as_bytes()[0])
                    && key.chars().all(|c| c.is_ascii_alphanumeric() || '_' == c) =>
            {
                write!(f, ".{key}")
            }
            Key(key) => write!(f, "['{key}']"),
        }
    }
}

impl GenericValue for Value {
    type Index = Index;

    fn children(&self) -> Vec<(Self::Index, &Self)> {
        use Value::*;

        match self {
            Null | Bool(_) | Number(_) | String(_) => Vec::new(),

            Array(array) => array
                .iter()
                .enumerate()
                .map(|(k, v)| (Index::Num(k), v))
                .collect(),
            Object(object) => object
                .iter()
                .map(|(k, v)| (Index::Key(k.clone()), v))
                .collect(),
        }
    }

    fn fmt_leaf(&self, f: &mut Formatter) -> FmtResult {
        use Value::*;

        match self {
            Null => write!(f, "\x1b[35mnull"),
            Bool(b) => write!(f, "\x1b[35m{b}"),
            Number(n) => write!(f, "\x1b[33m{n}"),
            String(s) => write!(f, "\x1b[32m\"{}\"", &s[..std::cmp::min(s.len(), 42)]),
            Array(a) => write!(f, "[{}] ", a.len()),
            Object(o) => write!(f, "{{{}}} ", o.len()),
        }
    }
}

impl Generic for Json {
    type Value = Value;

    fn root(&self) -> &Pin<Box<(Self::Value, PhantomPinned)>> {
        &self.0
    }
}

impl Json {
    pub fn new(path: &str) -> Result<Self, Error> {
        if path.is_empty() {
            serde_json::from_reader(io::stdin())
        } else {
            serde_json::from_reader(File::open(path).map_err(Error::IoErr)?)
        }
        .map(|value| Self(Box::pin((value, PhantomPinned))))
        .map_err(|_| todo!())
    }
}
