use std::cmp::Ordering;
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::path::Path;
use std::rc::Rc;

use sqlite::{self, Connection, OpenFlags};

use super::Error;
use crate::fisovec::FilterSorter;
use crate::tree::Provider;

pub struct Sqlite {
    connection: Rc<Connection>,
}

#[derive(PartialEq)]
pub struct Table {
    name: String,
}

#[derive(PartialEq)]
pub enum SqliteNode {
    Top,
    Table(Table),
}

impl Display for SqliteNode {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            SqliteNode::Top => write!(f, "*"),
            SqliteNode::Table(Table { name }) => write!(f, "{name}"),
        }
    }
}

impl Provider for Sqlite {
    type Fragment = SqliteNode;

    fn provide_root(&self) -> Self::Fragment {
        SqliteNode::Top
    }

    fn provide(&mut self, path: Vec<&Self::Fragment>) -> Vec<Self::Fragment> {
        // "pragma table_list" -> schema('main'|'temp'..), name, type('table'|'view'..), ncol, wr, strict
        // "pragma table_info(name)" -> name, type(?|''), notnull, dflt_value, pk(0 if not, or 1-based)
        if 1 == path.len() {
            self.connection
                .prepare("select name from sqlite_master where 'table' = type")
                .unwrap()
                .into_iter()
                .filter_map(Result::ok)
                .map(|row| {
                    SqliteNode::Table(Table {
                        name: row.read::<&str, _>("name").into(),
                    })
                })
                .collect()
        } else {
            Vec::new()
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
            (SqliteNode::Top, SqliteNode::Top) => unreachable!(),
            (SqliteNode::Table(a), SqliteNode::Table(b)) => Ord::cmp(&a.name, &b.name),
            _ => unreachable!(),
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
