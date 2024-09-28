use std::fmt::{Display, Formatter, Result as FmtResult};
use std::fs::File;
use std::io::{self, Read};
use std::marker::PhantomPinned;
use std::pin::Pin;

use toml::{self, Value};

use super::{
    generic::{Generic, GenericValue},
    Error,
};

pub struct Toml(Pin<Box<(Value, PhantomPinned)>>);

#[derive(Default, PartialEq, PartialOrd)]
pub enum Index {
    #[default]
    Root,
    Index(usize),
    Key(String),
}

impl Display for Index {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        match self {
            Index::Root => write!(f, "[]"),
            Index::Index(index) => write!(f, "{index}"),
            Index::Key(key) => write!(f, "{key}"),
        }
    }
}

impl GenericValue for Value {
    type Index = Index;

    fn children(&self) -> Vec<(Self::Index, &Self)> {
        use Value::*;

        match self {
            String(_) | Integer(_) | Float(_) | Boolean(_) | Datetime(_) => Vec::new(),

            Array(array) => array
                .iter()
                .enumerate()
                .map(|(k, v)| (Index::Index(k), v))
                .collect(),
            Table(table) => table
                .iter()
                .map(|(k, v)| (Index::Key(k.clone()), v))
                .collect(),
        }
    }

    fn fmt_leaf(&self, f: &mut Formatter) -> FmtResult {
        use Value::*;
        match self {
            String(s) => write!(f, "\x1b[32m\"{}\"", &s[..std::cmp::min(s.len(), 42)]),
            Integer(n) => write!(f, "\x1b[33m{n}"),
            Float(n) => write!(f, "\x1b[33m{n}"),
            Boolean(b) => write!(f, "\x1b[35m{b}"),
            Datetime(d) => write!(f, "\x1b[33m{d}"),
            Array(_) => Ok(()),
            Table(_) => Ok(()),
        }
    }
}

impl Generic for Toml {
    type Value = Value;

    fn root(&self) -> &Pin<Box<(Self::Value, PhantomPinned)>> {
        &self.0
    }
}

impl Toml {
    pub fn new(path: &str) -> Result<Self, Error> {
        let mut doc = String::new();
        if path.is_empty() {
            io::stdin().read_to_string(&mut doc)
        } else {
            File::open(path).and_then(|mut f| f.read_to_string(&mut doc))
        }
        .map_err(Error::IoErr)?;
        toml::from_str(&doc)
            .map(|value| Self(Box::pin((value, PhantomPinned))))
            .map_err(|_| todo!())
    }
}
