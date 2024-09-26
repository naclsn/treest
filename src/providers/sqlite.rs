use std::cmp::{self, Ordering};
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::iter;
use std::path::Path;
use std::rc::Rc;

use sqlite::{Connection, OpenFlags, Value};

use super::Error;
use crate::fisovec::FilterSorter;
use crate::tree::Provider;

pub struct Sqlite {
    connection: Rc<Connection>,
}

#[derive(PartialEq)]
pub enum SqliteNode {
    Top,
    Table {
        name: String,
    },
    Column {
        name: String,
        tipe: String,
        nullable: bool,
        default: Option<String>,
        iskey: bool,
    },
    Entries {
        count: usize,
    },
    Entry {
        pkey: usize,
        names: Vec<String>,
        values: Vec<Value>,
    },
    Field {
        name: String,
        value: Value,
    },
}
use SqliteNode::*;

impl Display for SqliteNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Top => write!(f, "*"),
            Table { name } => write!(f, "{name}"),
            Column {
                name,
                tipe,
                nullable,
                default,
                iskey,
            } => write!(
                f,
                "{}{name}\x1b[m: {tipe}{} = {default:?}",
                if *iskey { "\x1b[36m" } else { "" },
                if *nullable { '?' } else { '!' }
            ),
            Entries { count: 1 } => write!(f, "\x1b[34m1 entry\x1b[m"),
            Entries { count: n } => write!(f, "\x1b[34m{n} entries\x1b[m"),
            Entry { pkey, names, .. } => write!(f, "\x1b[36m{}\x1b[m", names[*pkey]),
            Field { name, value } => {
                write!(f, "{name}= ")?;
                match value {
                    Value::Binary(v) => {
                        write!(f, "\x1b[32m[")?;
                        for b in &v[..cmp::min(v.len(), 14)] {
                            write!(f, "{b:#02x}, ")?;
                        }
                        write!(f, "]\x1b[m")
                    }
                    Value::Float(d) => write!(f, "\x1b[33m{d}\x1b[m"),
                    Value::Integer(i) => write!(f, "\x1b[33m{i}\x1b[m"),
                    Value::String(s) => {
                        write!(f, "\x1b[32m\"{}\"\x1b[m", &s[..cmp::min(s.len(), 42)])
                    }
                    Value::Null => write!(f, "\x1b[35mnull\x1b[m"),
                }
            }
        }
    }
}

impl Provider for Sqlite {
    type Fragment = SqliteNode;

    fn provide_root(&self) -> Self::Fragment {
        Top
    }

    fn provide(&mut self, path: Vec<&Self::Fragment>) -> Vec<Self::Fragment> {
        match path[..] {
            [Top] => self
                .connection
                .prepare("select name from sqlite_master where 'table' = type")
                .unwrap()
                .into_iter()
                .filter_map(Result::ok)
                .map(|row| Table {
                    name: str::to_string(row.read("name")),
                })
                .collect(),

            [Top, Table { name }] => self
                .connection
                .prepare(format!("pragma table_info(\"{name}\")"))
                .unwrap()
                .into_iter()
                .filter_map(Result::ok)
                .map(|row| Column {
                    name: str::to_string(row.read("name")),
                    tipe: str::to_string(row.read("type")),
                    nullable: 0i64 == row.read("notnull"),
                    default: row
                        .try_read("dflt_value")
                        .unwrap_or(Some("?"))
                        .map(str::to_string),
                    iskey: 0i64 != row.read("pk"),
                })
                .chain(iter::once(Entries {
                    count: {
                        let mut x = self
                            .connection
                            .prepare(format!("select count(*) from \"{name}\""))
                            .unwrap();
                        x.next().unwrap();
                        x.read::<i64, _>(0).unwrap() as usize
                    },
                }))
                .collect(),

            [Top, Table { name }, Entries { .. }] => self
                .connection
                .prepare(format!("select * from \"{name}\" limit 12"))
                .unwrap()
                .into_iter()
                .filter_map(Result::ok)
                .map(|row| Entry {
                    pkey: 0,
                    names: vec![],
                    values: row.into(),
                })
                .collect(),

            [Top, Table { .. }, Entries { .. }, Entry { names, values, .. }] => {
                iter::zip(names.clone(), values.clone())
                    .map(|(name, value)| Field { name, value })
                    .collect()
            }

            _ => Vec::new(),
        }
    }
}

impl FilterSorter<<Self as Provider>::Fragment> for Sqlite {
    fn compare(
        &self,
        a: &<Self as Provider>::Fragment,
        b: &<Self as Provider>::Fragment,
    ) -> Ordering {
        match (a, b) {
            (Table { name: a }, Table { name: b }) => Ord::cmp(a, b),
            (Column { name: a, .. }, Column { name: b, .. }) => Ord::cmp(a, b),
            (Entries { count: _ }, Column { .. }) => Ordering::Greater,
            (Column { .. }, Entries { count: _ }) => Ordering::Less,
            _ => {
                let p = [a, b];
                let mut it = p.iter().map(|c| match c {
                    Top => "Top",
                    Table { .. } => "Table",
                    Column { .. } => "Column",
                    Entries { .. } => "Entries",
                    Entry { .. } => "Entry",
                    Field { .. } => "Field",
                });
                unreachable!("compare {} and {}", it.next().unwrap(), it.next().unwrap());
            }
        }
    }

    fn keep(&self, a: &<Self as Provider>::Fragment) -> bool {
        _ = a;
        true
    }
}

impl Sqlite {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, Error> {
        Ok(Self {
            connection: Connection::open_with_flags(path, OpenFlags::new().with_read_only())
                .map_err(|e| Error::StringErr(e.message.unwrap_or_default()))?
                .into(),
        })
    }
}
