use std::env;
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::fs;
use std::path::Path;

const PREL: &str = r##"
trait LuaTypeDoc {
    fn lua_type_doc() -> String;
}

macro_rules! impl_lua_type_doc {
    ($str:literal for $(< $($T:ident),* $(;$($W:ident),*)? $(;;$(const $N:ident: $nty:ty),*)? > $ty:ty),*$(,)*) => {
        $(impl<$($T: LuaTypeDoc),*$($(,$W)*)?$($(,const $N:$nty)*)?> LuaTypeDoc for $ty {
            #[inline]
            fn lua_type_doc() -> String {
                format!($str, $($T::lua_type_doc()),*)
            }
        })*
    };
    ($str:literal for $($ty:ty),*$(,)*) => {
        $(impl LuaTypeDoc for $ty {
            #[inline]
            fn lua_type_doc() -> String {
                format!($str)
            }
        })*
    };
    (any for $(<$T:ident: $sty:path> $ty:ty),*$(,)*) => {
        $(impl<$T: $sty> LuaTypeDoc for $ty {
            #[inline]
            fn lua_type_doc() -> String {
                format!("{}", std::any::type_name::<T>())
            }
        })*
    };
}

macro_rules! impl_lua_type_doc_tuples {
    ($($l:ident)+ @) => { };
    ($($l:ident)+ @ $h:ident $($t:ident)*) => {
        impl<$($l: LuaTypeDoc),+> LuaTypeDoc for ($($l,)+) {
            #[inline]
            fn lua_type_doc() -> String {
                format!("[{}]", [$($l::lua_type_doc()),+].join(", "))
            }
        }
        impl_lua_type_doc_tuples! { $($l)+ $h @ $($t)* }
    };
    ($h:ident $($t:ident)+) => {
        impl_lua_type_doc_tuples! { $h @ $($t)* __ }
    };
}

impl_lua_type_doc! { "any" for
    mlua::Value,
}
impl_lua_type_doc! { "boolean" for
    bool,
}
impl_lua_type_doc! { "error" for
    mlua::Error,
}
impl_lua_type_doc! { "function" for
    mlua::Function,
}
impl_lua_type_doc! { "integer" for
    i8, i16, i32, i64, i128, isize,
    u8, u16, u32, u64, u128, usize,
}
impl_lua_type_doc! { "lightuserdata" for
    mlua::LightUserData,
}
impl_lua_type_doc! { "nil" for
    (),
}
impl_lua_type_doc! { "number" for
    f32, f64,
}
impl_lua_type_doc! { "userdata" for
    mlua::UserData,
}
impl_lua_type_doc! { "string" for
    str, Box<str>, String, std::borrow::Cow<'_, str>,
    std::path::Path, std::path::PathBuf,
    std::ffi::CStr, std::ffi::CString, std::borrow::Cow<'_, std::ffi::CStr>,
    std::ffi::OsStr, std::ffi::OsString,
    bstr::BStr,
    mlua::String,
    mlua::BString,
}
impl_lua_type_doc! { "table" for
    mlua::Table,
}
impl_lua_type_doc! { "{} | {}" for
    <L, R> mlua::Either<L, R>,
}
impl_lua_type_doc! { "{}?" for
    <T> Option<T>,
}
impl_lua_type_doc! { "{}[]" for
    <T> &[T],
    <T;; const N: usize> [T; N],
    <T> Box<[T]>,
    <T> Vec<T>,
}
impl_lua_type_doc_tuples! { A B C D E F G H I J K L M N O P }
impl_lua_type_doc! { "table<{}, {}>" for
    <K, V; S> std::collections::HashMap<K, V, S>,
    <K, V> std::collections::BTreeMap<K, V>,
}
impl_lua_type_doc! { "{{ [{}]: boolean }}" for
    <T; S> std::collections::HashSet<T, S>,
    <T> std::collections::BTreeSet<T>,
}
impl_lua_type_doc! { any for
    <T: mlua::UserData> T,
    <T: mlua::IntoLua> T,
    <T: mlua::FromLua> T,
}
impl<T: LuaTypeDoc + ?Sized> LuaTypeDoc for &T {
    fn lua_type_doc() -> String {
        T::lua_type_doc()
    }
}
"##;

#[derive(Clone, Debug, Default, PartialEq)]
struct Export<'a> {
    table: Option<&'a str>,
    doc: Vec<&'a str>,
    name: &'a str,
    params: Vec<(&'a str, &'a str)>,
    ret: &'a str,
}

impl<'a> Export<'a> {
    fn parse(text: &'a str) -> Option<(Export<'a>, &'a str)> {
        let mut r = Self::default();

        let mut lines = text.lines().map(str::trim_start).peekable();

        let export_line = lines.find(|line| line.starts_with("/// Exported "))?;
        r.table = export_line
            .strip_prefix("/// Exported in ")
            .map(|s| &s[..s.len() - 1]); // remove trailing '.'

        while let Some(more) = lines.next_if(|line| line.starts_with("/// ")) {
            r.doc.push(&more[4..]);
        }
        let proto_line = lines.peek().and_then(|line| line.strip_prefix("fn "))?;

        let mut chars = proto_line.char_indices().peekable();

        r.name = &proto_line[chars.next()?.0..chars.find(|(_, c)| '(' == *c)?.0];

        if chars.next_if(|(_, c)| '&' == *c).is_some() {
            r.table = Some("treest");
            if 'm' == chars.peek()?.1 {
                chars.nth(8)?; // 'mut self'
            } else {
                chars.nth(4)?; // 'self'
            }
            if ',' == chars.peek()?.1 {
                chars.nth(2)?; // ', '
            }
        }

        while let Some((st, _)) = chars.peek().cloned().filter(|(_, c)| ')' != *c) {
            let ed = chars.find(|(_, c)| ':' == *c)?.0;
            let name = &proto_line[st..ed];

            let st = chars.nth(2)?.0; // ' '
            let ed = chars.find()?.0;
            let typ = &proto_line[st..ed];

            r.params.push((name, typ));
            if ',' == chars.peek()?.1 {
                chars.nth(2)?; // ', '
            }
        }

        let consumed = unsafe { proto_line.as_ptr().offset_from(text.as_ptr()) } as usize;
        Some((r, &text[consumed + proto_line.len() + 1..]))
    }
}

impl Display for Export<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        const INDE: &str = "";
        writeln!(f, "{INDE}Export {{")?;
        writeln!(f, "{INDE}    table: {:?},", self.table)?;
        writeln!(f, "{INDE}    name: {:?},", self.name)?;
        writeln!(f, "{INDE}    doc: &[")?;
        for line in &self.doc {
            writeln!(f, "{INDE}        {line:?}.to_string()")?;
        }
        writeln!(f, "{INDE}        String::new(),")?;
        for (name, typ) in &self.params {
            writeln!(
                f,
                r#"{INDE}        format!("@param {name} {{}}", <{typ}>::lua_type_doc()),"#
            )?;
        }
        if "()" != self.ret {
            writeln!(
                f,
                r#"{INDE}        format!("@return {{}}", <{}>::lua_type_doc()),"#,
                self.ret
            )?;
        }
        writeln!(f, "{INDE}    ],")?;
        write!(f, "{INDE}}}")
    }
}

#[test]
fn parse_and_export() {
    const IN: &str = r#"
stuff before

/// Exported globally.
/// Get a help text about a subject.
/// `help('help')` would return this text if it was actually implemented.
fn help(subj: String) -> Result<Option<String>> {
    ...
}

stuff after
"#;

    let ex = Export {
        table: None,
        doc: vec![
            "Get a help text about a subject.",
            "`help('help')` would return this text if it was actually implemented.",
        ],
        name: "help",
        params: vec![("subj", "String")],
        ret: "Option<String>",
    };

    assert_eq!(
        Export::parse(IN),
        Some((ex.clone(), "    ...\n}\n\nstuff after\n")),
    );

    assert_eq!(
        ex.to_string(),
        r##"Export {
    table: None,
    name: "help",
    doc: &[
        "Get a help text about a subject.".to_string()
        "`help('help')` would return this text if it was actually implemented.".to_string()
        String::new(),
        format!("@param subj {}", <String>::lua_type_doc()),
        format!("@return {}", <Option<String>>::lua_type_doc()),
    ],
}"##
    );
}

fn main() {
    //println!("{ex}");
    println!("cargo::error=stop");
}

fn main0() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=src/navigate/scripting.rs");

    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("help.rs");
    fs::write(
        &dest_path,
        r####"const HELP: &[(&str, &str)] = &[
    ("help", r###"Get a help text about a subject.
`help('help')` would return this text if it was actually implemented.

@param subj string
@return string?
"###),
];"####,
    )
    .unwrap();
}
