use std::io::{self, Read, Result as IoResult, Write};
use std::result::Result as StdResult;

use mlua::{Function, Lua, Result, Table, UserData, UserDataFields, UserDataMethods, Value};

use crate::navigate::{Navigate, Target, ViewJumpBy};
use crate::prompt::{self, PromptSplitInfo};
use crate::terminal;
use crate::tree::NodePath;

macro_rules! make_exports {
    ($t:expr, $lua:ident; $(pub $name:ident($($param:ident),*);)*) => {
        {
            let _t: &::mlua::Table = &$t;
            $(_t.raw_set(
                stringify!($name),
                $lua.create_function(|_, ($($param,)*)| $name($($param),*))?,
            )?;)*
        }
    };
}

macro_rules! make_methods {
    ($methods:ident; $($fn_mut:tt $name:ident($($param:ident),*);)*) => {
        $(make_methods!(@ $methods; $fn_mut $name($($param),*));)*
    };
    (@ $methods:ident; fn $name:ident(lua, $($param:ident),*)) => {
        $methods.add_method(stringify!($name), |lua, this, ($($param,)*)| this.$name(lua, $($param),*))
    };
    (@ $methods:ident; mut $name:ident(lua, $($param:ident),*)) => {
        $methods.add_method_mut(stringify!($name), |lua, this, ($($param,)*)| this.$name(lua, $($param),*))
    };
    (@ $methods:ident; fn $name:ident($($param:ident),*)) => {
        $methods.add_method(stringify!($name), |_, this, ($($param,)*)| this.$name($($param),*))
    };
    (@ $methods:ident; mut $name:ident($($param:ident),*)) => {
        $methods.add_method_mut(stringify!($name), |_, this, ($($param,)*)| this.$name($($param),*))
    };
}

impl UserData for Navigate {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("quitting", |_, nav| Ok(nav.exit.is_some()));
        fields.add_field_method_get("mouse_event_pos", |_, nav| {
            Ok(nav.input.get_pending_mouse_info())
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        make_methods! { methods;
            mut _atexit();
            mut _tick();
            mut enter();
            mut fold(target);
            fn  folded(target);
            fn  get_cursor();
            fn  get_option(lua, name); // TODO: remove this 'lua' special case
            fn  get_register(name);
            fn  get_register_hist(name);
            mut leave();
            mut map(seq, cb);
            mut mark(target);
            fn  marked(target);
            mut message(text);
            mut next(flags);
            mut unmark(target);
            mut prev(flags);
            mut prompt(ps, completion);
            fn  provider_name();
            mut quit(text);
            fn  search_deep(q, flags);
            fn  search_level(q, flags);
            mut set_cursor(target);
            mut set_option(name, value);
            mut set_register(name, value);
            mut suspend();
            mut unfold(target);
            mut view_down(by);
            mut view_up(by);
        }
    }
}

pub fn global_exports(g: &Table, lua: &Lua) -> Result<()> {
    make_exports! { g, lua;
        pub help(subj);
        pub keyseqstr(seq);
        pub keytrans(text);
        pub prompt(ps, history, completion);
    }
    make_exports! { g.get("string").unwrap(), lua;
        pub prompt_split(line, point);
    }
    make_exports! { g.get("debug").unwrap(), lua;
        pub pretty(obj);
    }
    Ok(())
}

crate::flags_lua_conversion!(MoveFlags {
    wrapping: "wrap" | "sat",
});
crate::flags_lua_conversion!(SearchFlags {
    wrapping: "wrap" | "sat",
    direction: "next" | "prev",
});
crate::flags_lua_conversion!(ScrollFlags {
    amount: "line" | "win" | "halfwin" | "mouse",
});

impl Navigate {
    fn _atexit(&mut self) -> Result<String> {
        let exit = self
            .exit
            .take()
            .unwrap_or("_atexit called too early (no exit text set)".into());
        self.term.take().map(|t| t.restore());
        Ok(exit)
    }

    fn _tick(&mut self) -> Result<Option<Function>> {
        // TODO: don't use Display, it will also remove the view: RefCell
        let buf = self.to_string();
        eprint!("{buf}");

        if let Some(ref mut s) = self.message {
            // TODO(maybe): --MORE-- prompt or something
            if let Some(n) = s.find('\n') {
                s.truncate(n);
            }
        }

        // rem: cannot call here beacause `self` is borrowed mut
        // (would cause a BadArgument: UserDataBorrowMutError)
        Ok(self.input.tick().cloned())
    }

    fn enter(&mut self) -> Result<()> {
        Ok(self.cursor_enter())
    }

    fn fold(&mut self, target: Target) -> Result<()> {
        self.set_folded(target, true);
        Ok(())
    }

    fn folded(&self, target: Target) -> Result<bool> {
        Ok(self.get_folded(target))
    }

    fn get_cursor(&self) -> Result<Vec<usize>> {
        Ok(self.cursor().to_vec())
    }

    fn get_option(&self, lua: &Lua, name: String) -> Result<Value> {
        Ok(self.options.get(&name, lua))
    }

    // TODO: remove the Option<>
    fn get_register(&self, name: String) -> Result<Option<String>> {
        Ok(self.registers.get(&name).and_then(|h| h.last()).cloned())
    }

    // TODO: remove the Option<>
    fn get_register_hist(&self, name: String) -> Result<Option<Vec<String>>> {
        Ok(self.registers.get(&name).cloned())
    }

    fn leave(&mut self) -> Result<()> {
        Ok(self.cursor_leave())
    }

    fn map(&mut self, seq: String, cb: Function) -> Result<()> {
        let seq = terminal::keytrans(seq.as_str()).expect("need valid seq something blbl TODO");
        self.input.add_mapping(seq, cb);
        Ok(())
    }

    fn mark(&mut self, target: Target) -> Result<()> {
        self.set_marked(target, true);
        Ok(())
    }

    fn marked(&self, target: Target) -> Result<bool> {
        Ok(self.get_marked(target))
    }

    fn next(&mut self, flags: MoveFlags) -> Result<()> {
        Ok(self.cursor_next("wrap" == flags.wrapping))
    }

    fn unmark(&mut self, target: Target) -> Result<()> {
        self.set_marked(target, false);
        Ok(())
    }

    /// (TODO: fix this) Exported in treest.
    /// Set the message text. If it spans on multiple lines,
    /// it will trigger the -- More -- prompt.
    fn message(&mut self, text: Option<String>) -> Result<()> {
        self.message = text.map(|w| w.replace("\n", "\r\n")); // TODO: somewhat of a temp hack
        Ok(())
    }

    fn prev(&mut self, flags: MoveFlags) -> Result<()> {
        Ok(self.cursor_prev("wrap" == flags.wrapping))
    }

    fn prompt(&mut self, ps: String, completion: Function) -> Result<Option<String>> {
        let history = self.registers.entry(ps.clone()).or_default();

        terminal::cursor_on();
        terminal::mouse_off();
        let ans = prompt::prompt(
            &ps,
            io::stdin().bytes().map_while(StdResult::ok),
            io::stderr(),
            history.clone(),
            |line, point| completion.call((line, point)).unwrap_or_default(),
        );
        terminal::cursor_off();
        terminal::mouse_on();

        ans.as_ref().inspect(|r| history.push(r.to_string()));
        Ok(ans)
    }

    fn provider_name(&self) -> Result<String> {
        Ok(self.provider_name.clone())
    }

    fn quit(&mut self, text: Option<String>) -> Result<()> {
        self.exit = text.or(Some(String::new()));
        Ok(())
    }

    fn search_deep(&self, q: String, flags: SearchFlags) -> Result<Option<Vec<usize>>> {
        todo!()
    }

    fn search_level(&self, q: String, flags: SearchFlags) -> Result<Option<Vec<usize>>> {
        if self.is_cursor_root() {
            return Ok(None);
        };
        let [parent_path @ .., current] = self.cursor() else {
            unreachable!();
        };
        let parent = self.tree.resolve(parent_path);
        let chs = parent.last().unwrap().children().unwrap();

        let Some(found) = slice_search(
            &chs,
            *current,
            |node| {
                self.provider
                    .display(&NodePath {
                        head: &parent,
                        tail: node,
                    })
                    .contains(&q)
            },
            "next" == flags.direction,
            "wrap" == flags.wrapping,
        ) else {
            return Ok(None);
        };

        let mut r = parent_path.to_vec();
        r.push(found);
        Ok(Some(r))
    }

    fn set_cursor(&mut self, target: Target) -> Result<()> {
        // TODO: check validity before assigning, unfolding and cropping as needed
        if let Target::Path(path) = target {
            self.cursor = (path.len(), path);
        }
        Ok(())
    }

    fn set_option(&mut self, name: String, value: Value) -> Result<()> {
        self.options.set(&name, value);
        Ok(())
    }

    fn set_register(&mut self, name: String, value: String) -> Result<()> {
        self.register_push(name, value);
        Ok(())
    }

    fn suspend(&mut self) -> Result<()> {
        #[cfg(not(windows))]
        {
            terminal::cursor_on();
            terminal::mouse_off();
            terminal::altscreen_off();

            self.term.take().map(|t| t.restore());
            unsafe { libc::raise(libc::SIGTSTP) };
            self.term = terminal::raw_with_panic_hook().ok();

            terminal::cursor_off();
            terminal::mouse_on();
            terminal::altscreen_on();
        }
        Ok(())
    }

    fn unfold(&mut self, target: Target) -> Result<()> {
        self.set_folded(target, false);
        Ok(())
    }

    fn view_down(&mut self, by: ScrollFlags) -> Result<()> {
        self.view.borrow_mut().down(match by.amount {
            "line" => ViewJumpBy::Line,
            "win" => ViewJumpBy::Win,
            "halfwin" => ViewJumpBy::HalfWin,
            "mouse" => ViewJumpBy::Mouse,
            _ => unreachable!(),
        });
        Ok(())
    }

    fn view_up(&mut self, by: ScrollFlags) -> Result<()> {
        self.view.borrow_mut().up(match by.amount {
            "line" => ViewJumpBy::Line,
            "win" => ViewJumpBy::Win,
            "halfwin" => ViewJumpBy::HalfWin,
            "mouse" => ViewJumpBy::Mouse,
            _ => unreachable!(),
        });
        Ok(())
    }
}

/// Exported globally.
/// Get a help text about a subject.
/// `help('help')` would return this text if it was actually implemented.
fn help(subj: String) -> Result<Option<String>> {
    if subj.is_empty() {
        Ok(Some(format!(
            "API items: {}",
            help::HELP
                .iter()
                .map(|ex| if let Some(table) = ex.table {
                    format!("{table}.{}", ex.name)
                } else {
                    format!("{}", ex.name)
                })
                .collect::<Vec<_>>()
                .join(", ")
        )))
    } else {
        Ok(help::HELP
            .iter()
            .find(|ex| ex.name.ends_with(&subj))
            .map(|ex| format!("{}", (ex.doc)().join("\n"))))
    }
}

// TODO: BString
fn keyseqstr(seq: Vec<u8>) -> Result<String> {
    Ok(terminal::keyseqstr(&seq))
}

// TODO: BString
fn keytrans(text: String) -> Result<Option<Vec<u8>>> {
    Ok(terminal::keytrans(&text))
}

/// Exported globally.
/// Pretty-print a value to string.
fn pretty(obj: Value) -> Result<String> {
    Ok(format!("{obj:#?}"))
}

/// Exported globally.
/// Prompt the user for a line of input.
/// Same as `treest:prompt` except the history must be managed manually
/// (the argument `history` isn't mutated).
fn prompt(ps: String, history: Vec<String>, completion: Function) -> Result<Option<String>> {
    terminal::cursor_on();
    terminal::mouse_off();
    let ans = prompt::prompt(
        &ps,
        io::stdin().bytes().map_while(StdResult::ok),
        io::stderr(),
        history.clone(),
        |line, point| completion.call((line, point)).unwrap_or_default(),
    );
    terminal::cursor_off();
    terminal::mouse_on();
    Ok(ans)
}

/// Exported in string.
/// Split a line of input in a shell-like manner.
fn prompt_split(line: String, point: Option<usize>) -> Result<PromptSplitInfo> {
    Ok(prompt::split(&line, point.unwrap_or_default()))
}

mod help {
    use super::*;
    include!(concat!(env!("OUT_DIR"), "/help.rs"));
}

pub fn gen_lua_meta(f: &mut impl Write) -> IoResult<()> {
    writeln!(f, "---@meta treest")?;
    writeln!(f)?;

    writeln!(f, "---@class treestlib")?;
    writeln!(f, "---@field quitting boolean")?;
    writeln!(
        f,
        "---@field mouse_event_pos {{ col: integer, row: integer }}"
    )?;
    writeln!(f, "treest = {{}}")?;

    for ex in help::HELP {
        writeln!(f)?;
        for line in (ex.doc)() {
            writeln!(f, "---{line}")?;
        }
        write!(f, "function ")?;
        if let Some(table) = ex.table {
            write!(f, "{table}.")?;
        }
        write!(f, "{}(", ex.name)?;
        let mut sep = "";
        for (name, typ) in (ex.params)() {
            write!(f, "{sep}{name}")?;
            sep = ", ";
        }
        writeln!(f, ") end")?;
    }

    Ok(())
}

fn slice_search<T>(
    slice: &[T],
    from: usize,
    mut predicate: impl FnMut(&T) -> bool,
    forward: bool,
    wrapping: bool,
) -> Option<usize> {
    if slice.is_empty() {
        return None;
    }

    let dir = if forward { 1 } else { slice.len() - 1 };
    match (forward, wrapping) {
        (_, true) => 1..slice.len() - 1,
        (true, false) => 1..slice.len() - (from + 1),
        (false, false) => 1..from + 1,
    }
    .map(|k| (from + k * dir) % slice.len())
    .find(|n| predicate(&slice[*n]))
}

#[cfg(test)]
mod test {
    #[test]
    fn slice_search() {
        assert_eq!(
            super::slice_search(b"abcdefg", 4, |c| b'f' == *c, true, false),
            Some(5),
        );
        assert_eq!(
            super::slice_search(b"abcdefg", 4, |c| b'b' == *c, true, false),
            None,
        );
        assert_eq!(
            super::slice_search(b"abcdefg", 4, |c| b'b' == *c, true, true),
            Some(1),
        );
        assert_eq!(
            super::slice_search(b"abcdefg", 4, |c| b'e' == *c, true, false),
            None,
        );
        assert_eq!(
            super::slice_search(b"abcdefg", 4, |c| b'e' == *c, true, true),
            None,
        );
        assert_eq!(
            super::slice_search(b"abcdefg", 4, |c| b'c' == *c, false, false),
            Some(2),
        );
        assert_eq!(
            super::slice_search(b"abcdefg", 4, |c| b'g' == *c, false, false),
            None,
        );
        assert_eq!(
            super::slice_search(b"abcdefg", 4, |c| b'g' == *c, false, true),
            Some(6),
        );
        assert_eq!(
            super::slice_search(b"abcdefg", 4, |c| b'e' == *c, false, false),
            None,
        );
        assert_eq!(
            super::slice_search(b"abcdefg", 4, |c| b'e' == *c, false, true),
            None,
        );
        assert_eq!(
            super::slice_search(b"ooxxoxoxx", 4, |c| b'o' == *c, false, false),
            Some(1),
        );
        assert_eq!(
            super::slice_search(b"ooxxoxoxx", 1, |c| b'o' == *c, false, false),
            Some(0),
        );
    }
}
