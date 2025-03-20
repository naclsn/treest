use std::io::{self, Read};
use std::result::Result as StdResult;

use mlua::{BString, Either, Function, Lua, Result, Table, Value};
use mlua::{UserData, UserDataFields, UserDataMethods};

use crate::lua::help;
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
            fn  mapped(seq);
            mut mark(target);
            fn  marked(target);
            mut message(text);
            mut next(flags);
            //fn  node(target); // TODO: need a proper return type to be added
            mut prev(flags);
            mut prompt(ps, completion);
            fn  provider_name();
            mut quit(text);
            mut search_deep(q, flags);
            fn  search_level(q, flags);
            mut set_cursor(target);
            mut set_option(name, value);
            mut set_register(name, value);
            mut suspend();
            mut unfold(target);
            mut unmap(seq);
            mut unmark(target);
            mut view_down(by);
            mut view_up(by);
        }
    }
}

pub fn global_exports(g: &Table, lua: &Lua) -> Result<()> {
    make_exports! { g, lua;
        pub help(subj);
        pub prompt(ps, history, completion);
    }
    make_exports! { g.get("string").unwrap(), lua;
        pub keyseqstr(seq);
        pub keytrans(text);
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
        if let Some(t) = self.term.take() {
            t.restore();
        }
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

    /// Exported in treest.
    /// Try to enter the node at cursor (ie relative motion), unfolding it as needed.
    /// Nothing happens if it cannot be unfolded.
    fn enter(&mut self) -> Result<()> {
        self.cursor_enter();
        Ok(())
    }

    /// Exported in treest.
    /// Fold the node at target (cursor if `nil`).
    /// Nothing happens if the target is not valid.
    fn fold(&mut self, target: Target) -> Result<()> {
        self.set_folded(target, true);
        Ok(())
    }

    /// Exported in treest.
    /// Check if the node at target (cursor if `nil`) is folded.
    /// Return `nil` if the target is not valid.
    fn folded(&self, target: Target) -> Result<Option<bool>> {
        Ok(self.get_folded(target))
    }

    /// Exported in treest.
    /// Retrieve the cursor path.
    /// Using this where a `Target` is expected is equivalent to `nil`.
    fn get_cursor(&self) -> Result<Vec<usize>> {
        Ok(self.cursor().to_vec())
    }

    // TODO: need to remove the 'lua'
    // Exported in treest.
    /// Get the value of an option.
    fn get_option(&self, lua: &Lua, name: String) -> Result<Value> {
        Ok(self.options.get(&name, lua))
    }

    // TODO: remove the Option<>
    /// Exported in treest.
    /// Get the value of a register.
    fn get_register(&self, name: String) -> Result<Option<String>> {
        Ok(self.registers.get(&name).and_then(|h| h.last()).cloned())
    }

    // TODO: remove the Option<>
    /// Exported in treest.
    /// Get the values taken by a register,
    /// including the current on which will be the last one.
    fn get_register_hist(&self, name: String) -> Result<Option<Vec<String>>> {
        Ok(self.registers.get(&name).cloned())
    }

    /// Exported in treest.
    /// Try to leave the node at cursor (ie relative motion).
    /// Nothing happens if the cursor is already at root node.
    fn leave(&mut self) -> Result<()> {
        self.cursor_leave();
        Ok(())
    }

    /// Exported in treest.
    /// Add a mapping from a key sequence to a callback action.
    /// See also `treest:unmap`.
    fn map(&mut self, seq: String, cb: Function) -> Result<()> {
        let seq = terminal::keytrans(seq.as_str()).expect("need valid seq something blbl TODO");
        self.input.add_mapping(seq, cb);
        Ok(())
    }

    /// Exported in treest.
    /// Retrieve a mapping from a key sequence, returning its action callback.
    /// Result will be `nil` if `seq` wasn't mapped (see `treest:map`).
    fn mapped(&self, seq: String) -> Result<Option<Function>> {
        let seq = terminal::keytrans(seq.as_str()).expect("need valid seq something blbl TODO");
        Ok(self.input.get_mapping(seq).cloned())
    }

    /// Exported in treest.
    /// Mark the node at target (cursor if `nil`).
    /// Nothing happens if the target is not valid.
    fn mark(&mut self, target: Target) -> Result<()> {
        self.set_marked(target, true);
        Ok(())
    }

    /// Exported in treest.
    /// Check if the node at target (cursor if `nil`) is marked.
    /// Return `nil` if the target is not valid.
    fn marked(&self, target: Target) -> Result<Option<bool>> {
        Ok(self.get_marked(target))
    }

    /// Exported in treest.
    /// Move cursor to the next sibling node (ie relative motion).
    /// When flag is 'sat' and it's the last child, nothing happens.
    /// 'wrap' will instead go back to first child.
    fn next(&mut self, flags: MoveFlags) -> Result<()> {
        self.cursor_next("wrap" == flags.wrapping);
        Ok(())
    }

    /// Exported in treest.
    /// Retrieve node information at target (cursor if `nil`) or `nil` if the path is not valid.
    /// TODO: proper return like NodeInfo or something.
    fn node(&self, target: Target) -> Result<Option<(String, Option<usize>)>> {
        Ok(self.resolve_node(&target).map(|node| {
            (
                "node".to_string(), // TODO: ofc
                if node.is_loaded() {
                    Some(node.child_count())
                } else {
                    None
                },
            )
        }))
    }

    /// Exported in treest.
    /// Remove a mapping from a key sequence, returning its previously associated action callback.
    /// Result will be `nil` if `seq` wasn't mapped (see `treest:mapped`).
    fn unmap(&mut self, seq: String) -> Result<Option<Function>> {
        let seq = terminal::keytrans(seq.as_str()).expect("need valid seq something blbl TODO");
        Ok(self.input.pop_mapping(seq))
    }

    /// Exported in treest.
    /// Unmark the node at target (cursor if `nil`).
    fn unmark(&mut self, target: Target) -> Result<()> {
        self.set_marked(target, false);
        Ok(())
    }

    /// Exported in treest.
    /// Set the message text. If it spans on multiple lines,
    /// it will trigger the -- More -- prompt.
    fn message(&mut self, text: Option<Either<String, Vec<String>>>) -> Result<()> {
        self.message = text.map(|w| {
            match w {
                Either::Left(s) => s.split("\n").map(String::from).collect(),
                Either::Right(l) => l,
            }
            .join("\r\n") // TODO: temp hack until -- More --
        });
        Ok(())
    }

    /// Exported in treest.
    /// Move cursor to the previous sibling node (ie relative motion).
    /// When flag is 'sat' and it's the first child, nothing happens.
    /// 'wrap' will instead go back to last child.
    fn prev(&mut self, flags: MoveFlags) -> Result<()> {
        self.cursor_prev("wrap" == flags.wrapping);
        Ok(())
    }

    /// Exported in treest.
    /// Prompt the user for a line of input.
    /// The result is stored in the register given by `ps`.
    /// History is also taken from the previous values of the register.
    /// See also `treest:set_register` for direct access.
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

    /// Exported in treest.
    /// Retrieve the name of the current provider (eg. 'fs').
    fn provider_name(&self) -> Result<String> {
        Ok(self.provider_name.clone())
    }

    /// Exported in treest.
    /// Quit the application.
    /// If the text is non-empty, it is considered an error message to be printed and the exit code will be 1.
    /// Only `nil` is considered a normal exit situation.
    fn quit(&mut self, text: Option<String>) -> Result<()> {
        self.exit = text.or(Some(String::new()));
        Ok(())
    }

    /// Exported in treest.
    /// Not implemented yet.
    /// Search for a node with `q` in its text.
    /// The search is performed depth-first, greedily unfolding nodes as needed.
    fn search_deep(&mut self, _q: String, _flags: SearchFlags) -> Result<Option<Vec<usize>>> {
        todo!()
    }

    /// Exported in treest.
    /// Search for a sibling node with `q` in its text.
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

    /// Exported in treest.
    /// Move the cursor the the given target.
    /// If the target is not valid, the longest valid path is used (for now we just crash).
    fn set_cursor(&mut self, target: Target) -> Result<()> {
        // TODO: check validity before assigning, unfolding and cropping as needed
        if let Target::Path(path) = target {
            self.cursor = (path.len(), path);
        }
        Ok(())
    }

    /// Exported in treest.
    /// Set the value of an option.
    fn set_option(&mut self, name: String, value: Value) -> Result<()> {
        self.options.set(&name, value);
        Ok(())
    }

    /// Exported in treest.
    /// Set the value of a register.
    /// Registers are also set when using `treest:prompt`.
    fn set_register(&mut self, name: String, value: String) -> Result<()> {
        self.register_push(name, value);
        Ok(())
    }

    /// Exported in treest.
    /// Suspend execution for job control (by raising a SIGTSTP).
    /// This is like hitting <C-Z> on most terminal programs.
    /// It is a no-op under Windows.
    fn suspend(&mut self) -> Result<()> {
        #[cfg(not(windows))]
        {
            terminal::cursor_on();
            terminal::mouse_off();
            terminal::altscreen_off();

            if let Some(t) = self.term.take() {
                t.restore();
            }
            unsafe { libc::raise(libc::SIGTSTP) };
            self.term = terminal::raw_with_panic_hook().ok();

            terminal::cursor_off();
            terminal::mouse_on();
            terminal::altscreen_on();
        }
        Ok(())
    }

    /// Exported in treest.
    /// Unfold the node at target (cursor if `nil`).
    fn unfold(&mut self, target: Target) -> Result<()> {
        self.set_folded(target, false);
        Ok(())
    }

    /// Exported in treest.
    /// Move the view down, revealing any hidden lines at the bottom.
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

    /// Exported in treest.
    /// Move the view up, revealing any hidden lines at the top.
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
            "API items:{}",
            help::HELP
                .iter()
                .map(|ex| if let Some(table) = ex.table {
                    format!(" {table}.{}", ex.name)
                } else {
                    format!(" {}", ex.name)
                })
                .collect::<String>()
        )))
    } else if let Some(ex) = help::HELP.iter().find(|ex| subj == ex.name) {
        Ok(Some(ex.doc.join("\n")))
    } else {
        Ok(None)
    }
}

/// Exported in string.
/// Translate a byte string back to a key sequence: `somestr:keyseqstr():keytrans() == somestr`.
fn keyseqstr(seq: BString) -> Result<String> {
    Ok(terminal::keyseqstr(&seq))
}

/// Exported in string.
/// Translate a key sequence into the corresponding byte string.
/// Note that `treest:map` expects a non-translated string! (Tho the result will be the same.)
///
/// Notations are mostly taken from Vim. Here are the recognised forms:
/// ```text
/// <Nul> <BS> <Tab> <NL> <CR> <Space> <lt> <gt> <Bslash> <Bar> <CSI>
/// <Up> <Down> <Right> <Left>
/// <Home> <End> <Insert> <Delete> <PageUp> <PageDown>
/// <LeftMouse> <RightMouse> <ForwardWheel> <BackwardWheel> <UpMouse>
/// <C-..> <M-..> <A-..>
/// ```
fn keytrans(text: String) -> Result<Option<BString>> {
    Ok(terminal::keytrans(&text).map(BString::from))
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
