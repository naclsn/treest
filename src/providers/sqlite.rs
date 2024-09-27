use std::cmp::{self, Ordering};
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::iter;
use std::path::Path;

use sqlite::{Connection, OpenFlags, Value};

use super::Error;
use crate::fisovec::FilterSorter;
use crate::tree::{Provider, ProviderExt};

pub struct Sqlite {
    connection: Connection,
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
        default: Option<Value>,
        iskey: bool,
    },
    Entries {
        count: usize,
        pkey: usize,
        names: Vec<String>,
    },
    More {
        more: usize,
    },
    Entry {
        pkey: usize,
        values: Vec<Value>,
    },
    Field {
        name: String,
        value: Value,
    },
}
use SqliteNode::*;

fn write_value(f: &mut Formatter<'_>, value: &Value) -> FmtResult {
    use Value::*;
    match value {
        Binary(v) => {
            write!(f, "\x1b[32m[")?;
            for b in &v[..cmp::min(v.len(), 14)] {
                write!(f, "{b:#02x}, ")?;
            }
            write!(f, "]\x1b[m")
        }
        Float(d) => write!(f, "\x1b[33m{d}\x1b[m"),
        Integer(i) => write!(f, "\x1b[33m{i}\x1b[m"),
        String(s) => {
            write!(f, "\x1b[32m\"{}\"\x1b[m", &s[..cmp::min(s.len(), 42)])
        }
        Null => write!(f, "\x1b[35mnull\x1b[m"),
    }
}

fn compare_values(a: &Value, b: &Value) -> Ordering {
    use Value::*;
    match (a, b) {
        (Binary(a), Binary(b)) => Ord::cmp(a, b),
        (Float(a), Float(b)) => PartialOrd::partial_cmp(a, b).unwrap_or(Ordering::Equal),
        (Integer(a), Integer(b)) => Ord::cmp(a, b),
        (String(a), String(b)) => Ord::cmp(a, b),
        //(Binary(_), _) => todo!(),
        //(Float(_), _) => todo!(),
        //(Integer(_), _) => todo!(),
        //(String(_), _) => todo!(),
        //(Null, _) => todo!(),
        _ => Ordering::Equal,
    }
}

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
            } => {
                write!(
                    f,
                    "{}{name}\x1b[m\x1b[37m: {tipe}{}\x1b[m",
                    if *iskey { "\x1b[36m" } else { "" },
                    if *nullable { '?' } else { '!' }
                )?;
                if let Some(value) = default {
                    write!(f, "\x1b[37m= ")?;
                    write_value(f, value)?;
                }
                Ok(())
            }

            Entries { count: 1, .. } => write!(f, "\x1b[34m1 entry\x1b[m "),
            Entries { count: n, .. } => write!(f, "\x1b[34m{n} entries\x1b[m"),

            Entry { pkey, values, .. } => {
                write_value(f, &values[*pkey])?;
                write!(f, " ")
            }

            More { more } => write!(f, "\x1b[34m... (+{more})\x1b[m"),

            Field { name, value } => {
                write!(f, "{name}\x1b[m\x1b[37m= ")?;
                write_value(f, value)
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
        const LIMIT: usize = 42;

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

            [Top, Table { name }] => {
                let (names, mut r): (Vec<_>, Vec<_>) = self
                    .connection
                    .prepare(format!("pragma table_info(\"{name}\")"))
                    .unwrap()
                    .into_iter()
                    .filter_map(Result::ok)
                    .map(|mut row| {
                        let name = str::to_string(row.read("name"));
                        (
                            name.clone(),
                            Column {
                                name,
                                tipe: str::to_string(row.read("type")),
                                nullable: 0i64 == row.read("notnull"),
                                default: match row.take("dflt_value") {
                                    Value::Null => None,
                                    value => Some(value),
                                },
                                iskey: 0i64 != row.read("pk"),
                            },
                        )
                    })
                    .unzip();
                r.push(Entries {
                    count: {
                        let mut x = self
                            .connection
                            .prepare(format!("select count(*) from \"{name}\""))
                            .unwrap();
                        x.next().unwrap();
                        x.read::<i64, _>(0).unwrap() as usize
                    },
                    pkey: r
                        .iter()
                        .position(|w| matches!(w, Column { iskey: true, .. }))
                        .unwrap_or(0),
                    names,
                });
                r
            }

            [Top, Table { name }, Entries {
                count, pkey, names, ..
            }] => {
                let mut r: Vec<_> = self
                    .connection
                    .prepare(format!(
                        "select * from \"{name}\" order by \"{}\" limit {LIMIT}",
                        names[*pkey]
                    ))
                    .unwrap()
                    .into_iter()
                    .filter_map(Result::ok)
                    .map(|row| Entry {
                        pkey: *pkey,
                        values: row.into(),
                    })
                    .collect();
                if LIMIT < *count {
                    r.push(More {
                        more: *count - LIMIT,
                    });
                }
                r
            }

            [Top, Table { name }, Entries {
                count, pkey, names, ..
            }, .., More { more }] => {
                let mut r: Vec<_> = self
                    .connection
                    .prepare(format!(
                        "select * from \"{name}\" order by \"{}\" limit {LIMIT} offset {}",
                        names[*pkey],
                        count - more
                    ))
                    .unwrap()
                    .into_iter()
                    .filter_map(Result::ok)
                    .map(|row| Entry {
                        pkey: *pkey,
                        values: row.into(),
                    })
                    .collect();
                if LIMIT < *more {
                    r.push(More { more: more - LIMIT });
                }
                r
            }

            [Top, Table { .. }, Entries { pkey, names, .. }, Entry { values, .. }] => {
                iter::zip(names.clone(), values.clone())
                    .enumerate()
                    .filter_map(|(k, (name, value))| {
                        if *pkey != k {
                            Some(Field { name, value })
                        } else {
                            None
                        }
                    })
                    .collect()
            }

            _ => Vec::new(),
        }
    }
}

impl ProviderExt for Sqlite {}

impl FilterSorter<<Self as Provider>::Fragment> for Sqlite {
    fn compare(
        &self,
        a: &<Self as Provider>::Fragment,
        b: &<Self as Provider>::Fragment,
    ) -> Ordering {
        match (a, b) {
            (Table { name: a }, Table { name: b }) => Ord::cmp(a, b),

            (Column { name: a, .. }, Column { name: b, .. }) => Ord::cmp(a, b),
            (Entries { .. }, Column { .. }) => Ordering::Greater,
            (Column { .. }, Entries { .. }) => Ordering::Less,

            (
                Entry {
                    pkey: k, values: a, ..
                },
                Entry { values: b, .. },
            ) => compare_values(&a[*k], &b[*k]),
            (More { .. }, Entry { .. }) => Ordering::Greater,
            (Entry { .. }, More { .. }) => Ordering::Less,

            (Field { name: a, .. }, Field { name: b, .. }) => Ord::cmp(a, b),

            _ => {
                let p = [a, b];
                let mut it = p.iter().map(|c| match c {
                    Top => "Top",
                    Table { .. } => "Table",
                    Column { .. } => "Column",
                    Entries { .. } => "Entries",
                    More { .. } => "More",
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
                .map_err(|e| Error::StringErr(e.message.unwrap_or_default()))?,
        })
    }
}
