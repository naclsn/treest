use std::cmp::Ordering;
use std::fs::File;
use std::path::Path;

use anyhow::Result;
use serde_json::{self, Value};

use crate::provider::Provider;
use crate::tree::{Fragment, Node, NodePath};

pub struct Json(Value);

enum Frag {
    Root,
    Index(usize),
    Key(String),
}

fn resolve<'a, 'b>(mut val: &'a Value, path: impl Iterator<Item = &'b Node>) -> &'a Value {
    for frag in path.map(Node::fragment) {
        val = match frag {
            &Frag::Root => val,
            &Frag::Index(k) => &val.as_array().unwrap()[k],
            Frag::Key(k) => &val.as_object().unwrap()[k],
        };
    }
    val
}

impl Provider for Json {
    fn provide_root(&self) -> Fragment {
        Box::new(Frag::Root)
    }

    fn provide(&mut self, path: &NodePath) -> Vec<Fragment> {
        match resolve(&self.0, path.iter_all()) {
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => Vec::new(),
            Value::Array(values) => (0..values.len())
                .map(|k| Box::new(Frag::Index(k)) as Fragment)
                .collect(),
            Value::Object(map) => map
                .keys()
                .map(|k| Box::new(Frag::Key(k.clone())) as Fragment)
                .collect(),
        }
    }

    fn order(&self, left: &NodePath, right: &NodePath) -> Ordering {
        match (left.tail.fragment(), right.tail.fragment()) {
            (Frag::Index(l), Frag::Index(r)) => Ord::cmp(l, r),
            (Frag::Key(l), Frag::Key(r)) => Ord::cmp(l, r),
            _ => unreachable!(),
        }
    }

    fn keep(&self, _path: &NodePath) -> bool {
        true
    }

    fn display(&self, path: &NodePath) -> String {
        let k = match path.tail.fragment() {
            Frag::Root => "\x1b[35m$\x1b[m".to_string(),
            Frag::Index(k) => format!("\x1b[33m{k}\x1b[m"),
            Frag::Key(k) => format!("\x1b[34m{k:?}\x1b[m"),
        };
        match resolve(&self.0, path.iter_all()) {
            Value::Null => format!("{k}: \x1b[34mnull"),
            Value::Bool(b) => format!("{k}: \x1b[34m{b}"),
            Value::Number(n) => format!("{k}: \x1b[33m{n}"),
            Value::String(s) => {
                let ss = &s[..s.char_indices().nth(42).map(|p| p.0).unwrap_or(s.len())];
                format!("{k}: \x1b[37m(x{})\x1b[32m{:?}", s.len(), ss)
            }
            Value::Array(a) => format!("{k}: \x1b[37m[x{}]", a.len()),
            Value::Object(o) => format!("{k}: \x1b[37m{{x{}}}", o.len()),
        }
    }

    fn components(&self, path: &NodePath) -> Vec<String> {
        path.iter_all()
            .map(|n| match n.fragment() {
                Frag::Root => "$".to_string(),
                Frag::Index(k) => format!("[{k}]"),
                Frag::Key(k) => match k.as_bytes() {
                    [b'A'..=b'Z' | b'_' | b'a'..=b'z', rest @ ..]
                        if !rest
                            .iter()
                            .any(|&c| !c.is_ascii_alphanumeric() && b'_' != c) =>
                    {
                        format!(".{k}")
                    }
                    _ => format!("[{k:?}]"),
                },
            })
            .collect()
    }

    fn join(&self, components: &[String]) -> String {
        components.join("")
    }

    fn split<'a>(&self, _path: &'a NodePath<'a>, _text: String) -> Result<(Vec<&'a Node>, Fragment)> {
        todo!()
    }
}

impl Json {
    pub fn new(file: impl AsRef<Path>) -> Result<Self> {
        Ok(Self(serde_json::from_reader(File::open(file)?)?))
    }

    pub fn guess(arg: &str) -> bool {
        arg.ends_with(".json")
    }
}
