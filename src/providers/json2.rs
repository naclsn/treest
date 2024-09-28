use std::fmt::{Display, Formatter, Result as FmtResult};
use std::fs::File;
use std::io::Read;
use std::iter::{self, Peekable};
use std::marker::PhantomPinned;
use std::path::Path;
use std::pin::Pin;

use super::{
    generic::{Generic, GenericValue},
    Error,
};

pub struct Json2(Pin<Box<(Value, PhantomPinned)>>);

pub enum Value {
    Null,
    Boolean(bool),
    Number(String),
    String(String),
    Array(Vec<Value>),
    Object(Vec<(String, Value)>),
}

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
            Index::Root => write!(f, "$"),
            Index::Index(index) => write!(f, "[{index}]"),
            // TODO: provider-specific setting to show all keys as in JSON (ie. `"key"`)
            Index::Key(key)
                if (key.as_bytes()[0].is_ascii_alphabetic() || b'_' == key.as_bytes()[0])
                    && key.chars().all(|c| c.is_ascii_alphanumeric() || '_' == c) =>
            {
                write!(f, ".{key}")
            }
            Index::Key(key) => write!(f, "['{key}']"),
        }
    }
}

impl GenericValue for Value {
    type Index = Index;

    fn children(&self) -> Vec<(Self::Index, &Self)> {
        use Value::*;

        match self {
            Null | Boolean(_) | Number(_) | String(_) => Vec::new(),

            Array(array) => array
                .iter()
                .enumerate()
                .map(|(k, v)| (Index::Index(k), v))
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
            Boolean(b) => write!(f, "\x1b[35m{b}"),
            Number(n) => write!(f, "\x1b[33m{n}"),
            String(s) => write!(f, "\x1b[32m\"{}\"", &s[..std::cmp::min(s.len(), 42)]),
            Array(a) => write!(f, "[{}] ", a.len()),
            Object(o) => write!(f, "{{{}}} ", o.len()),
        }
    }
}

impl Generic for Json2 {
    type Value = Value;

    fn root(&self) -> &Pin<Box<(Self::Value, PhantomPinned)>> {
        &self.0
    }
}

// parsing {{{
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

impl Json2 {
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

        Ok(Self(Box::pin((
            Json2::parse(&mut lex.peekable()).map_err(Error::ParseErr)?,
            PhantomPinned,
        ))))
    }

    fn parse(lex: &mut Peekable<impl Iterator<Item = (usize, Token)>>) -> Result<Value, usize> {
        use Token::*;
        let (pos, tok) = lex.next().ok_or(usize::MAX)?;
        match tok {
            OpenBracket => Json2::parse_array(lex),
            OpenBrace => Json2::parse_object(lex),
            Null => Ok(Value::Null),
            True => Ok(Value::Boolean(true)),
            False => Ok(Value::Boolean(false)),
            Number(n) => Ok(Value::Number(n)),
            String(s) => Ok(Value::String(s)),
            Comma | Colon | CloseBracket | CloseBrace | Unknown => Err(pos),
        }
    }

    fn parse_array(
        lex: &mut Peekable<impl Iterator<Item = (usize, Token)>>,
    ) -> Result<Value, usize> {
        let mut array = Vec::new();
        if !matches!(lex.peek(), Some((_, Token::CloseBracket))) {
            loop {
                array.push(Json2::parse(lex)?);
                match lex.next().ok_or(usize::MAX)? {
                    (_, Token::Comma) => continue,
                    (_, Token::CloseBracket) => break,
                    (pos, _) => return Err(pos),
                }
            }
        } else {
            lex.next();
        }
        Ok(Value::Array(array))
    }

    fn parse_object(
        lex: &mut Peekable<impl Iterator<Item = (usize, Token)>>,
    ) -> Result<Value, usize> {
        let mut object = Vec::new();
        if !matches!(lex.peek(), Some((_, Token::CloseBrace))) {
            loop {
                object.push((
                    match lex.next().ok_or(usize::MAX)? {
                        (_, Token::String(key)) => key,
                        (pos, _) => return Err(pos),
                    },
                    match lex.next().ok_or(usize::MAX)? {
                        (_, Token::Colon) => Json2::parse(lex)?,
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
        Ok(Value::Object(object))
    }
}
// }}}
