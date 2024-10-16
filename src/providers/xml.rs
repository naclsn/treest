use std::fmt::{Display, Formatter, Result as FmtResult};
use std::fs::File;
use std::io::{self, Read};
use std::marker::PhantomPinned;
use std::pin::Pin;

use anyhow::Result;
use xml::attribute::OwnedAttribute;
use xml::name::OwnedName;
use xml::reader::{EventReader, XmlEvent};

use super::generic::{Generic, GenericValue};

pub enum Node {
    Element {
        name: OwnedName,
        attributes: Vec<OwnedAttribute>,
        children: Vec<Node>,
    },
    Text(String),
}

pub struct Xml(Pin<Box<(Node, PhantomPinned)>>);

#[derive(Default, PartialEq, PartialOrd)]
pub enum Index {
    #[default]
    Document,
    Nth(usize, Option<String>),
}

impl Display for Index {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        use Index::*;

        match self {
            Document => write!(f, " "),
            Nth(_, Some(s)) => write!(f, "/{s}"),
            Nth(n, _) => write!(f, "[{n}]"),
        }
    }
}

impl GenericValue for Node {
    type Index = Index;

    fn children(&self) -> Vec<(Self::Index, &Self)> {
        use Node::*;

        match self {
            Element { children, .. } => children
                .iter()
                .enumerate()
                .map(|(k, v)| {
                    (
                        Index::Nth(
                            k,
                            match v {
                                Element { name, .. } => Some(name.local_name.clone()),
                                Text(_) => None,
                            },
                        ),
                        v,
                    )
                })
                .collect(),
            Text(_) => Vec::new(),
        }
    }

    fn fmt_leaf(&self, f: &mut Formatter) -> FmtResult {
        use Node::*;

        match self {
            Element { attributes, .. } => attributes.iter().try_for_each(|it| write!(f, "{it} ")),
            Text(s) => write!(f, "\x1b[32m\"{}\"", &s[..std::cmp::min(s.len(), 42)]),
        }
    }
}

impl Generic for Xml {
    type Value = Node;

    fn root(&self) -> &Pin<Box<(Self::Value, PhantomPinned)>> {
        &self.0
    }
}

impl Xml {
    pub fn new(path: &str) -> Result<Self> {
        Ok(if path.is_empty() {
            Node::from_reader(io::stdin())
        } else {
            Node::from_reader(File::open(path)?)
        }
        .map(|value| Self(Box::pin((value, PhantomPinned))))?)
    }
}

impl Node {
    fn from_reader<R: Read>(source: R) -> Result<Self> {
        let mut stack = Vec::new();

        for e in EventReader::new(source) {
            use XmlEvent::*;

            match e? {
                StartDocument { .. } => continue,
                EndDocument => unreachable!(),

                StartElement {
                    name, attributes, ..
                } => stack.push(Node::Element {
                    name,
                    attributes,
                    children: Vec::new(),
                }),

                EndElement { .. } => {
                    let finished = stack.pop().unwrap();
                    if let Some(Node::Element { children, .. }) = stack.last_mut() {
                        children.push(finished);
                    } else {
                        return Ok(finished);
                    }
                }

                Characters(text) => {
                    if let Some(Node::Element { children, .. }) = stack.last_mut() {
                        children.push(Node::Text(text));
                    }
                }

                ProcessingInstruction { .. } | CData(_) | Comment(_) | Whitespace(_) => (),
            }
        }

        unreachable!()
    }
}
