use std::io::{Result as IoResult, Write};

use mlua::{BString, Either, Value};

use crate::lua::structs::*;
use crate::lua::typedoc::{LuaTypeAliasDoc, LuaTypeDoc};

type ExportParamNameAndType = (&'static str, fn() -> String);
pub struct Export {
    pub doc: &'static [&'static str],
    pub table: Option<&'static str>,
    pub name: &'static str,
    pub params: &'static [ExportParamNameAndType],
    pub ret: fn() -> String,
}

pub const HELP: &[Export] = &include!(concat!(env!("OUT_DIR"), "/help.rs"));

impl Export {
    pub fn gen_lua_meta(&self, f: &mut impl Write) -> IoResult<()> {
        for line in self.doc {
            writeln!(f, "---{line}")?;
        }
        let ret = (self.ret)();
        if !self.params.is_empty() || "nil" != ret {
            writeln!(f, "---")?;
        }

        for (name, typ) in self.params {
            writeln!(f, "---@param {name} {}", typ())?;
        }
        if "nil" != ret {
            writeln!(f, "---@return {}", ret)?;
        }

        write!(f, "function ")?;
        if let Some(table) = self.table {
            write!(f, "{table}{}", if "treest" == table { ":" } else { "." })?;
        }
        write!(f, "{}(", self.name)?;
        let mut sep = "";
        for (name, _) in self.params {
            write!(f, "{sep}{name}")?;
            sep = ", ";
        }
        writeln!(f, ") end")
    }
}

macro_rules! top_aliases {
    ($f:ident; $($ty:ty),*$(,)?) => {
        $({
            let alias = <$ty>::lua_type_doc();
            let expand = <$ty>::lua_type_doc_alias_to();
            writeln!($f, "---@alias {alias} {expand}")?;
        })*
    };
}

pub fn gen_lua_meta(f: &mut impl Write) -> IoResult<()> {
    writeln!(f, "---@meta treest")?;
    writeln!(f)?;

    top_aliases!(f;
        MoveFlags,
        NodeInfo,
        PendingMouseInfo,
        PromptSplitInfo,
        ProviderFlags,
        ScrollFlags,
        SearchFlags,
        Target,
    );

    writeln!(f)?;
    writeln!(f, "---@class treestlib")?;
    writeln!(f, "---@field quitting boolean")?;
    writeln!(f, "---@field mouse_event_pos PendingMouseInfo")?;
    writeln!(f, "treest = {{}}")?;

    for ex in crate::lua::help::HELP {
        writeln!(f)?;
        ex.gen_lua_meta(f)?;
    }

    Ok(())
}
