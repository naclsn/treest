use std::cmp::Ordering;
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::fs::File;
use std::io::Read;
use std::iter::{self, Peekable};
use std::path::Path;

use super::Error;
use crate::fisovec::FilterSorter;
use crate::tree::Provider;

pub struct Json {
    json: JsonValue,
}

enum JsonValue {
    Null,
    Boolean(bool),
    Number(String),
    String(String),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
}

#[derive(PartialEq)]
pub enum JsonPathFragment {
    Root,
    Index(usize),
    Key(String),
}

#[derive(PartialEq)]
pub enum JsonPathTo {
    Null,
    Boolean(bool),
    Number(String),
    String(String),
    Array,
    Object,
}

#[derive(PartialEq)]
pub struct JsonNode {
    fragment: JsonPathFragment,
    to: JsonPathTo,
}

impl Display for JsonNode {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        use JsonPathFragment::*;
        use JsonPathTo::*;

        match &self.fragment {
            Root => write!(f, "\x1b[34m."),
            Index(index) => write!(f, "\x1b[34m{index}"),
            Key(key) => write!(f, "\x1b[34m\"{key}\""),
        }?;

        write!(f, "\x1b[m: ")?;

        match &self.to {
            Null => write!(f, "\x1b[35mnull\x1b[m"),
            Boolean(b) => write!(f, "\x1b[35m{b}\x1b[m"),
            Number(n) => write!(f, "\x1b[33m{n}\x1b[m"),
            String(s) => write!(f, "\x1b[32m\"{}\"\x1b[m", &s[..std::cmp::min(s.len(), 42)]),
            Array => write!(f, "[] "),
            Object => write!(f, "{{}} "),
        }
    }
}

impl Provider for Json {
    type Fragment = JsonNode;

    fn provide_root(&self) -> Self::Fragment {
        JsonNode {
            fragment: JsonPathFragment::Root,
            to: self.json.as_to(),
        }
    }

    fn provide(&mut self, path: Vec<&Self::Fragment>) -> Vec<Self::Fragment> {
        let Some(node) = self.json.resolve(path) else {
            return Vec::new();
        };

        use JsonPathFragment::*;
        use JsonValue::*;

        match node {
            Null | Boolean(_) | Number(_) | String(_) => Vec::new(),

            Array(array) => array
                .iter()
                .enumerate()
                .map(|(k, v)| JsonNode {
                    fragment: Index(k),
                    to: v.as_to(),
                })
                .collect(),

            Object(object) => object
                .iter()
                .map(|(k, v)| JsonNode {
                    fragment: Key(k.clone()),
                    to: v.as_to(),
                })
                .collect(),
        }
    }
}

impl FilterSorter<JsonNode> for Json {
    fn compare(&self, a: &JsonNode, b: &JsonNode) -> Ordering {
        use JsonPathFragment::*;
        match (&a.fragment, &b.fragment) {
            (Index(a), Index(b)) => Ord::cmp(a, b),
            (Key(a), Key(b)) => Ord::cmp(a, b),
            _ => Ordering::Equal,
        }
    }

    fn keep(&self, a: &JsonNode) -> bool {
        _ = a;
        true
    }
}

#[derive(Debug)]
enum Token {
    Comma,
    Colon,
    OpenBracket,
    CloseBracket,
    OpenBrace,
    CloseBrace,
    Null,
    True,
    False,
    Number(String),
    String(String),
    Unknown,
}

impl Json {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, Error> {
        let mut bytes = File::open(path)
            .map_err(Error::IoErr)?
            .bytes()
            .map_while(Result::ok)
            .enumerate()
            .peekable();

        let lex = iter::from_fn(move || {
            use Token::*;

            let (pos, ch) = bytes.by_ref().find(|p| !p.1.is_ascii_whitespace())?; // '\f'
            let mut ifexact = |lit: &[u8], r| {
                if bytes
                    .by_ref()
                    .take(lit.len() - 1)
                    .zip(lit[1..].iter())
                    .all(|(a, b)| a.1 == *b)
                {
                    r
                } else {
                    Unknown
                }
            };

            Some((
                pos,
                match ch {
                    b',' => Comma,
                    b':' => Colon,
                    b'{' => OpenBrace,
                    b'}' => CloseBrace,
                    b'[' => OpenBracket,
                    b']' => CloseBracket,
                    b'n' => ifexact(b"null", Null),
                    b't' => ifexact(b"true", True),
                    b'f' => ifexact(b"false", False),

                    // TODO: https://www.json.org/json-en.html
                    n if n.is_ascii_digit() => Number(
                        std::string::String::from_utf8(
                            iter::once(n)
                                .chain(iter::from_fn(|| {
                                    bytes
                                        .by_ref()
                                        .next_if(|p| p.1.is_ascii_digit())
                                        .map(|p| p.1)
                                }))
                                .collect(),
                        )
                        .unwrap(),
                    ),

                    // TODO: https://www.json.org/json-en.html
                    b'"' => String({
                        let mut escape = false;
                        std::string::String::from_utf8(
                            bytes
                                .by_ref()
                                .map_while(|p| {
                                    if escape {
                                        escape = false;
                                    } else {
                                        match p.1 {
                                            b'\\' => escape = true,
                                            b'"' => return None,
                                            _ => (),
                                        }
                                    }
                                    Some(p.1)
                                })
                                .collect(),
                        )
                        .unwrap()
                    }),

                    _ => Unknown,
                },
            ))
        });

        Ok(Self {
            json: Json::parse(&mut lex.peekable()).map_err(Error::ParseErr)?,
        })
    }

    fn parse(lex: &mut Peekable<impl Iterator<Item = (usize, Token)>>) -> Result<JsonValue, usize> {
        use Token::*;
        let (pos, tok) = lex.next().ok_or(usize::MAX)?;
        match tok {
            OpenBracket => Json::parse_array(lex),
            OpenBrace => Json::parse_object(lex),
            Null => Ok(JsonValue::Null),
            True => Ok(JsonValue::Boolean(true)),
            False => Ok(JsonValue::Boolean(false)),
            Number(n) => Ok(JsonValue::Number(n)),
            String(s) => Ok(JsonValue::String(s)),
            Comma | Colon | CloseBracket | CloseBrace | Unknown => Err(pos),
        }
    }

    fn parse_array(
        lex: &mut Peekable<impl Iterator<Item = (usize, Token)>>,
    ) -> Result<JsonValue, usize> {
        let mut array = Vec::new();
        if !matches!(lex.peek(), Some((_, Token::CloseBracket))) {
            loop {
                array.push(Json::parse(lex)?);
                match lex.next().ok_or(usize::MAX)? {
                    (_, Token::Comma) => continue,
                    (_, Token::CloseBracket) => break,
                    (pos, _) => return Err(pos),
                }
            }
        } else {
            lex.next();
        }
        Ok(JsonValue::Array(array))
    }

    fn parse_object(
        lex: &mut Peekable<impl Iterator<Item = (usize, Token)>>,
    ) -> Result<JsonValue, usize> {
        let mut object = Vec::new();
        if !matches!(lex.peek(), Some((_, Token::CloseBrace))) {
            loop {
                object.push((
                    match lex.next().ok_or(usize::MAX)? {
                        (_, Token::String(key)) => key,
                        (pos, _) => return Err(pos),
                    },
                    match lex.next().ok_or(usize::MAX)? {
                        (_, Token::Colon) => Json::parse(lex)?,
                        (pos, _) => return Err(pos),
                    },
                ));
                match lex.next().ok_or(usize::MAX)? {
                    (_, Token::Comma) => continue,
                    (_, Token::CloseBrace) => break,
                    (pos, _) => return Err(pos),
                }
            }
        } else {
            lex.next();
        }
        Ok(JsonValue::Object(object))
    }
}

impl JsonValue {
    fn resolve<'a>(&'a self, path: impl IntoIterator<Item = &'a JsonNode>) -> Option<&'a Self> {
        path.into_iter().try_fold(self, |m, f| m.resolve_one(f))
    }

    fn resolve_one(&self, path: &JsonNode) -> Option<&Self> {
        use JsonPathFragment::*;
        use JsonValue::*;

        match (self, &path.fragment) {
            (any, Root) => Some(any),
            (Array(array), Index(index)) => array.get(*index),
            (Object(object), Key(key)) => object.iter().find(|p| key == &p.0).map(|p| &p.1),
            _ => None,
        }
    }

    fn as_to(&self) -> JsonPathTo {
        use JsonValue::*;

        match self {
            Null => JsonPathTo::Null,
            Boolean(b) => JsonPathTo::Boolean(*b),
            Number(n) => JsonPathTo::Number(n.clone()),
            String(s) => JsonPathTo::String(s.clone()),
            Array(_) => JsonPathTo::Array,
            Object(_) => JsonPathTo::Object,
        }
    }
}
