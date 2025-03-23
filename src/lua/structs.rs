use std::ops::{Deref, DerefMut};

use mlua::{FromLua, IntoLua, Lua, Result, Value};

use crate::lua::typedoc::{LuaTypeAliasDoc, LuaTypeDoc};

crate::struct_lua_conversion! {
    pub struct PendingMouseInfo {
        pub col: u8,
        pub row: u8,
    }

    pub struct NodeInfo {
        pub name: String,
        pub components: Vec<String>,
        pub breadcrumbs: String,
        pub child_count: Option<usize>,
        pub path: IndexPath,
    }

    pub struct PromptSplitInfo {
        pub args: Vec<String>,
        pub in_arg: usize,
    }
}

crate::flags_lua_conversion! {
    pub struct MoveFlags {
        pub wrapping: "wrap" | "sat",
    }

    pub struct SearchFlags {
        pub wrapping: "wrap" | "sat",
        pub direction: "next" | "prev",
    }

    pub struct ScrollFlags {
        pub amount: "line" | "win" | "halfwin" | "mouse",
    }
}

pub use _index_path::IndexPath;
mod _index_path {
    use super::*;

    #[derive(Clone, Debug, Default)]
    pub struct IndexPath(Vec<usize>);

    impl From<Vec<usize>> for IndexPath {
        fn from(value: Vec<usize>) -> Self {
            Self(value)
        }
    }

    impl From<&[usize]> for IndexPath {
        fn from(value: &[usize]) -> Self {
            Self(value.to_vec())
        }
    }

    impl Deref for IndexPath {
        type Target = Vec<usize>;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl DerefMut for IndexPath {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }

    impl FromLua for IndexPath {
        fn from_lua(value: Value, lua: &Lua) -> Result<Self> {
            let mut v = Vec::from_lua(value, lua)?;
            v.iter_mut().for_each(|k| *k -= 1);
            Ok(Self(v))
        }
    }

    impl IntoLua for IndexPath {
        fn into_lua(mut self, lua: &Lua) -> Result<Value> {
            self.0.iter_mut().for_each(|k| *k += 1);
            self.0.into_lua(lua)
        }
    }

    impl LuaTypeDoc for IndexPath {
        fn lua_type_doc() -> String {
            "integer[]".to_string()
        }
    }
}

pub use _target::Target;
mod _target {
    use super::*;

    #[derive(Debug, Clone)]
    pub enum Target {
        Cursor,
        Path(IndexPath),
        #[allow(dead_code)] // m keepin it for now
        TrustedPath(IndexPath),
    }

    impl FromLua for Target {
        fn from_lua(value: Value, lua: &Lua) -> Result<Self> {
            match value {
                Value::Nil => Ok(Target::Cursor),
                _ => Ok(Target::Path(IndexPath::from_lua(value, lua)?)),
            }
        }
    }

    impl LuaTypeDoc for Target {
        fn lua_type_doc() -> String {
            "Target".to_string()
        }
    }

    impl LuaTypeAliasDoc for Target {
        fn lua_type_doc_alias_to() -> String {
            "integer[]?".to_string()
        }
    }
}
