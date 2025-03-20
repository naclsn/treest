/// This build script runs through src/navigate/scripting.rs to parse and collect function
/// definitions marked with "/// Exported". It then generates a `pub const HELP: &[Export]` which
/// is raw-include!-ed in src/lua/help.rs. This constant can then be used to retrieve doc text and
/// type info for a particular function or to generate the whole ---@meta lua file.
///
/// The detection/parsing is extremely minimal and expects the following as of now:
/// ```
/// $( )*/// Exported $(globally|in $name)?.
/// $( )*$(/// $docline)*
/// $( )*fn $name($(&self, |&mut self, )?$($pname: $ptyp), *) -> Result<$ret> {
/// ```
use std::env;
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::fs::{self, File};
use std::io::{Result as IoResult, Write};
use std::path::Path;

#[derive(Clone, Debug, Default, PartialEq)]
struct Export<'a> {
    doc: Vec<&'a str>,
    table: Option<&'a str>,
    name: &'a str,
    params: Vec<(&'a str, &'a str)>,
    ret: &'a str,
}

impl<'a> Export<'a> {
    fn parse(text: &'a str) -> Option<(Export<'a>, &'a str)> {
        let mut r = Self::default();

        let mut lines = text
            .lines()
            //.inspect(|l| eprintln!("{l:?}"))
            .map(str::trim_start)
            .peekable();
        eprintln!("-- find '/// Exported '");

        let export_line = lines.find(|line| line.starts_with("/// Exported "))?;
        r.table = export_line
            .strip_prefix("/// Exported in ")
            .map(|s| &s[..s.len() - 1]); // remove trailing '.'
        eprintln!("-- ok");

        while let Some(more) = lines.next_if(|line| line.starts_with("/// ")) {
            r.doc.push(&more[4..]);
        }
        let proto_line = lines.peek().and_then(|line| line.strip_prefix("fn "))?;
        eprintln!("-- proto_line: {proto_line:?}");

        let mut chars = proto_line.char_indices().peekable();

        r.name = &proto_line[chars.next()?.0..chars.find(|(_, c)| '(' == *c)?.0];
        eprintln!("   name: {:?}", r.name);

        if chars.next_if(|(_, c)| '&' == *c).is_some() {
            let mutable = 'm' == chars.peek()?.1;
            if mutable {
                chars.nth(7)?; // 'mut self'
            } else {
                chars.nth(3)?; // 'self'
            }
            if ',' == chars.peek()?.1 {
                chars.nth(1)?; // ', '
            }
            eprintln!("   < {}mutable self", if mutable { "" } else { "im" });
        }

        while let Some((st, _)) = chars
            .peek()
            .cloned()
            .filter(|(_, c)| ')' != *c && '-' != *c)
        {
            let name = &proto_line[st..chars.find(|(_, c)| ':' == *c)?.0];
            eprintln!("   / name: {name:?}");

            let st = chars.nth(1)?.0; // ' '
            let mut stack = Vec::new();
            let ed = chars
                .find(|(_, c)| {
                    match c {
                        '(' => stack.push(')'),
                        '[' => stack.push(']'),
                        '<' | '-' => stack.push('>'),
                        ',' | ')' if stack.is_empty() => return true,
                        _ if Some(c) == stack.last() => _ = stack.pop(),
                        _ => (),
                    }
                    false
                })?
                .0;
            let typ = &proto_line[st..ed];
            eprintln!("   \\ typ: {typ:?}");

            r.params.push((name, typ));
            chars.next();
        }

        if ')' == chars.peek()?.1 {
            chars.nth(1)?;
        }
        let l = proto_line.len();
        r.ret = &proto_line[chars.next()?.0 + 10..l - 3]; //  '-> Result<' and '> {'

        let consumed = unsafe { proto_line.as_ptr().offset_from(text.as_ptr()) } as usize;
        eprintln!(
            "-- success {:?}.{:?}/{} :: {:?}",
            r.table,
            r.name,
            r.params.len(),
            r.ret,
        );
        Some((r, &text[consumed + l + 1..]))
    }
}

impl Display for Export<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        const INDE: &str = "    ";
        writeln!(f, "{INDE}Export {{")?;
        writeln!(f, "{INDE}    doc: &[")?;
        for line in &self.doc {
            writeln!(f, "{INDE}        {line:?},")?;
        }
        writeln!(f, "{INDE}    ],")?;
        writeln!(f, "{INDE}    table: {:?},", self.table)?;
        writeln!(f, "{INDE}    name: {:?},", self.name)?;
        writeln!(f, "{INDE}    params: &[")?;
        for (name, typ) in &self.params {
            writeln!(f, r#"{INDE}        ({name:?}, <{typ}>::lua_type_doc),"#)?;
        }
        writeln!(f, "{INDE}    ],")?;
        writeln!(f, "{INDE}    ret: <{}>::lua_type_doc,", self.ret)?;
        write!(f, "{INDE}}}")
    }
}

fn main() -> IoResult<()> {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=src/navigate/scripting.rs");

    let out_dir = env::var_os("OUT_DIR").unwrap_or(".".into());

    let scripting = fs::read_to_string("src/navigate/scripting.rs")?;
    let mut helprs = File::create(Path::new(&out_dir).join("help.rs"))?;

    let mut head = scripting.as_str();
    writeln!(helprs, "[")?;
    while let Some((ex, ahead)) = Export::parse(head) {
        writeln!(helprs, "{ex},")?;
        head = ahead;
    }
    writeln!(helprs, "]")?;

    Ok(())
}
