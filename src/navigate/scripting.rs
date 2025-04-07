use std::fs::File;
use std::io::{self, Read};
use std::result::Result as StdResult;

use mlua::{AnyUserData, MetaMethod, UserData, UserDataFields, UserDataMethods};
use mlua::{BString, Either, Error, Function, Lua, Result, Table, Value};

use crate::lua::help;
use crate::lua::structs::{Completion, PromptAnsCallback};
use crate::lua::structs::{IndexPath, Listing, NodeInfo, PromptSplitInfo, Target};
use crate::lua::structs::{MoveFlags, RequestFlags, ScrollFlags, SearchFlags};
use crate::navigate::input::InputTickResponse;
use crate::navigate::options::Options;
use crate::navigate::{Navigate, ViewJumpBy};
use crate::prompt;
use crate::terminal::{self, KeyTransError};
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

        $methods.add_meta_function(MetaMethod::Pairs, |lua, this: Value| {
            let mut names = [$(stringify!($name)),*].iter();

            let next = lua.create_function_mut(move |_, this: AnyUserData| {
                if let Some(&name) = names.next() {
                    let index: Function = this.metatable()?.get("__index")?;
                    let value: Value = index.call((this, name))?;
                    Ok((Some(name), Some(value)))
                } else {
                    Ok((None, None))
                }
            })?;

            Ok((next, this))
        });
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
            fn  get_option(name);
            fn  get_register(name);
            fn  get_register_hist(name);
            mut leave();
            fn  list_options();
            fn  list_registers();
            mut map(seq, cb);
            fn  mapped(seq);
            mut mark(target);
            fn  marked(target);
            mut message(text);
            mut message_scroll_down(by);
            mut message_scroll_up(by);
            mut next(flags);
            fn  node(target);
            fn  node_at_line(line);
            mut prev(flags);
            mut prompt(ps, completion, then);
            fn  provider_name();
            mut provider_request(req, target, text);
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
    }
    make_exports! { g.get("os").unwrap(), lua;
        pub list(dir);
    }
    make_exports! { g.get("string").unwrap(), lua;
        pub keyseqstr(seq);
        pub keytrans(text);
        pub prompt_lua_tokens_split(line, point);
        pub prompt_shell_like_split(line, point);
    }
    make_exports! { g.get("debug").unwrap(), lua;
        pub pretty(obj);
    }
    Ok(())
}

impl Navigate {
    fn _atexit(&mut self) -> Result<String> {
        let exit = self
            .exit
            .take()
            .unwrap_or("_atexit called too early (no exit text set)".into());

        if let Some(mut dir) = dirs::cache_dir() {
            dir.push("treest.hist");
            if let Ok(mut f) = File::create(dir) {
                _ = self.save_registers(&mut f);
            }
        }

        if let Some(t) = self.term.take() {
            t.restore();
        }

        Ok(exit)
    }

    fn _tick(&mut self) -> Result<Option<Function>> {
        if let Err(err) = self.render_buffered(&mut io::stderr(), false) {
            // assume unrecoverable situation, bail out
            self.exit = Some(err.to_string());
            return Ok(None);
        }

        Ok(match self.input.tick() {
            InputTickResponse::Noop => None,
            InputTickResponse::EndOfInput => {
                // assume unrecoverable situation, bail out
                self.exit = Some("EOF".to_string());
                None
            }
            // rem: cannot call here beacause `self` is borrowed mut
            // (would cause a BadArgument: UserDataBorrowMutError)
            InputTickResponse::Callback(action) => Some(action),
            InputTickResponse::CallbackWithArg(prompt_cb, prompt_ans) => todo!(),
        })
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

    /// Exported in treest.
    /// Get the value of an option.
    /// See `treest:list_options` for a list of the available option names.
    fn get_option(&self, name: String) -> Result<Either<String, Either<isize, bool>>> {
        self.options.get(&name).ok_or(Error::BadArgument {
            to: Some("get_option".to_string()),
            pos: 2,
            name: Some("name".to_string()),
            cause: Error::RuntimeError(format!("no option {name:?}")).into(),
        })
    }

    /// Exported in treest.
    /// Get the value of a register or nil if it doesn't exist.
    fn get_register(&self, name: String) -> Result<Option<String>> {
        Ok(self.registers.get(&name).and_then(|h| h.last()).cloned())
    }

    /// Exported in treest.
    /// Get the values taken by a register or nil if it doesn't exist.
    /// This includes the current value which will be the last entry.
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
    /// List the available options (*names* only).
    fn list_options(&self) -> Result<Vec<String>> {
        Ok(Options::list().iter().map(|s| s.to_string()).collect())
    }

    /// Exported in treest.
    /// List the non-empty registers (*names* only).
    fn list_registers(&self) -> Result<Vec<String>> {
        Ok(self.registers.keys().cloned().collect())
    }

    /// Exported in treest.
    /// Add a mapping from a key sequence to a callback action.
    /// See also `treest:unmap`.
    fn map(&mut self, seq: String, cb: Function) -> Result<()> {
        let seq = terminal::keytrans(&seq).map_err(|err| transpose_keytranserror(&seq, err))?;
        self.input.add_mapping(seq, cb);
        Ok(())
    }

    /// Exported in treest.
    /// Retrieve a mapping from a key sequence, returning its action callback.
    /// Result will be `nil` if `seq` wasn't mapped (see `treest:map`).
    fn mapped(&self, seq: String) -> Result<Option<Function>> {
        let seq = terminal::keytrans(&seq).map_err(|err| transpose_keytranserror(&seq, err))?;
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
    fn node(&self, target: Target) -> Result<Option<NodeInfo>> {
        Ok(self.retrieve_node_info(target))
    }

    /// Exported in treest.
    /// Retrieve node information at a given display line (or 'row'), or `nil` if there is none.
    fn node_at_line(&self, line: usize) -> Result<Option<NodeInfo>> {
        Ok(self
            .view
            .path_for(line)
            .and_then(|path| self.retrieve_node_info(Target::TrustedPath(path.clone()))))
    }

    /// Exported in treest.
    /// Remove a mapping from a key sequence, returning its previously associated action callback.
    /// Result will be `nil` if `seq` wasn't mapped (see `treest:mapped`).
    fn unmap(&mut self, seq: String) -> Result<Option<Function>> {
        let seq = terminal::keytrans(&seq).map_err(|err| transpose_keytranserror(&seq, err))?;
        Ok(self.input.pop_mapping(seq))
    }

    /// Exported in treest.
    /// Unmark the node at target (cursor if `nil`).
    fn unmark(&mut self, target: Target) -> Result<()> {
        self.set_marked(target, false);
        Ok(())
    }

    /// Exported in treest.
    /// Set the message text. A multiline message can be interacted with through
    /// `treest:message_scroll_up` and `treest:message_scroll_down`.
    /// Setting to an empty vector will essentially clear it.
    fn message(&mut self, text: Option<Either<String, Vec<String>>>) -> Result<()> {
        if let Some(text) = text {
            self.set_message_lines(match text {
                Either::Left(s) => s.lines().map(String::from).collect(),
                Either::Right(l) => l,
            });
        }
        Ok(())
    }

    /// Exported in treest.
    /// Move the message view down, revealing any hidden lines at the bottom.
    fn message_scroll_down(&mut self, by: ScrollFlags) -> Result<()> {
        let msh = self.options.messageheight as usize;
        ViewJumpBy::new_by(by.amount).down(
            &mut self.message.scroll,
            msh,
            self.message.lines.len().saturating_sub(msh - 1),
        );
        Ok(())
    }

    /// Exported in treest.
    /// Move the message view up, revealing any hidden lines at the top.
    fn message_scroll_up(&mut self, by: ScrollFlags) -> Result<()> {
        ViewJumpBy::new_by(by.amount).up(
            &mut self.message.scroll,
            self.options.messageheight as usize,
            0,
        );
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
    /// The `point` argument to the `completion` function is 0-base.
    /// See also `treest:set_register` for direct register access.
    fn prompt(
        &mut self,
        ps: String,
        completion: Completion,
        then: Option<PromptAnsCallback>,
    ) -> Result<Option<String>> {
        terminal::cursor(true);
        if !self.options.mouse {
            terminal::mouse(false);
        }

        let history = self.register_entries(&ps);
        let ans = prompt::prompt(
            &ps,
            io::stdin().bytes().map_while(StdResult::ok),
            history.clone(),
            Box::new(move |line: &str, point| {
                completion
                    .call(line, point)
                    .unwrap_or_default()
                    .unwrap_or_default()
            }),
        );

        terminal::cursor(false);
        if self.options.mouse {
            terminal::mouse(true);
        }
        eprint!("\r\x1b[K");

        if let Some(thing) = &ans {
            self.register_push(&ps, thing.clone());
        }
        Ok(ans)
    }

    /// Exported in treest.
    /// Retrieve the name of the current provider (eg. 'fs').
    fn provider_name(&self) -> Result<String> {
        Ok(self.provider_name.clone())
    }

    /// Exported in treest.
    /// Execute a provider request at target (cursor if `nil`).
    /// It will be interpreted in a provider-specific way.
    /// `text` is not relevant and not used with `'rm'` and `'vi'`.
    /// The result is only (potentially) relevant with `'vi'` and `'ex'`.
    fn provider_request(
        &mut self,
        req: RequestFlags,
        target: Target,
        text: Option<String>,
    ) -> Result<Option<Vec<String>>> {
        let path = self.tree.resolve(&self.target_to_path(target));
        let path = &path[..].into();

        if !matches!(req.request, "rm" | "vi") && text.is_none() {
            // copied to match mlua's `FromLua for String`
            return Err(Error::FromLuaConversionError {
                from: "nil",
                to: "string".to_string(),
                message: Some("expected string or number".to_string()),
            });
        }

        match req.request {
            "mk" => self.provider.request_mk(path, text.unwrap()),
            "cp" => self.provider.request_cp(path, text.unwrap()),
            "rm" => self.provider.request_rm(path),
            "mv" => self.provider.request_mv(path, text.unwrap()),
            "ch" => self.provider.request_ch(path, text.unwrap()),
            _ => {
                return match req.request {
                    "vi" => self.provider.request_vi(path),
                    "ex" => self.provider.request_ex(path, text.unwrap()),
                    _ => unreachable!(),
                }
                .map_err(Error::external)
                .map(Some)
            }
        }
        .map_err(Error::external)
        .map(|()| None)
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
    fn search_level(&self, q: String, flags: SearchFlags) -> Result<Option<IndexPath>> {
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
        Ok(Some(r.into()))
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
    /// See `treest:list_options` for a list of the available option names.
    fn set_option(
        &mut self,
        name: String,
        value: Either<String, Either<isize, bool>>,
    ) -> Result<()> {
        self.options.set(&name, value).ok_or(Error::BadArgument {
            to: Some("set_option".to_string()),
            pos: 2,
            name: Some("name".to_string()),
            cause: Error::RuntimeError(format!("no option {name:?}")).into(),
        })
    }

    /// Exported in treest.
    /// Set the value of a register.
    /// Registers are also set when using `treest:prompt`.
    fn set_register(&mut self, name: String, value: String) -> Result<()> {
        self.register_push(&name, value);
        Ok(())
    }

    /// Exported in treest.
    /// Suspend execution for job control (by raising a SIGTSTP).
    /// This is like hitting `<C-Z>` on most terminal programs.
    /// It is a no-op under Windows.
    fn suspend(&mut self) -> Result<()> {
        #[cfg(not(windows))]
        {
            terminal::cursor(true);
            if self.options.mouse {
                terminal::mouse(false);
            }
            if self.options.altscreen {
                terminal::altscreen(false);
            }

            if let Some(t) = self.term.take() {
                t.restore();
            }
            unsafe { libc::raise(libc::SIGTSTP) };
            self.term = terminal::raw_with_panic_hook().ok();

            terminal::cursor(false);
            if self.options.mouse {
                terminal::mouse(true);
            }
            if self.options.altscreen {
                terminal::altscreen(true);
            }
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
        ViewJumpBy::new_by(by.amount).view_down(&mut self.view);
        Ok(())
    }

    /// Exported in treest.
    /// Move the view up, revealing any hidden lines at the top.
    fn view_up(&mut self, by: ScrollFlags) -> Result<()> {
        ViewJumpBy::new_by(by.amount).view_up(&mut self.view);
        Ok(())
    }
}

/// Exported globally.
/// Get a help text about a subject. `help('help')` returns this text.
/// The special subject `'*'` lists other subject.
fn help(subj: Option<String>) -> Result<Option<String>> {
    Ok(match subj.unwrap_or_default().as_str() {
        "" => "hi :3

to get help about a subject, use `:help <subject>`
or call the `help(<subject>)` lua function

`*` is a special subject that lists other subjects

(TODO: flesh this help out)"
            .to_string()
            .into(),

        "*" => {
            let functions = help::HELP.iter().map(|ex| format!("{}()", ex.name));
            let options = Options::list().iter().map(|op| format!("'{op}'"));
            let mut all: Vec<_> = functions.chain(options).collect();
            all.sort_unstable();
            Some(all.join("\n") + "\n")
        }

        f if f.ends_with("()") || f.ends_with("(") => {
            let Some(ex) = help::HELP.iter().find(|ex| ex.name.starts_with(f)) else {
                return Ok(None);
            };
            let mut r = ex.doc.join("\n") + "\n";
            let ret = (ex.ret)();
            if !ex.params.is_empty() || "nil" != ret {
                r += "\n";
            }
            for (name, typ) in ex.params {
                r += &format!("@param {name} {}\n", typ());
            }
            if "nil" != ret {
                r += &format!("@return {}\n", ret);
            }
            Some(r)
        }

        o if o.starts_with("'") => {
            Options::help(&o.strip_suffix("'").unwrap_or(o)[1..]).map(|ln| ln.join("\n") + "\n")
        }

        _ => None,
    })
}

/// Exported in string.
/// Translate a byte string back to a key sequence: `somestr:keyseqstr():keytrans() == somestr`.
fn keyseqstr(bytes: BString) -> Result<String> {
    Ok(terminal::keyseqstr(&bytes))
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
fn keytrans(seq: String) -> Result<BString> {
    terminal::keytrans(&seq)
        .map_err(|err| transpose_keytranserror(&seq, err))
        .map(BString::from)
}

/// Exported in os.
/// Return an iterator function that lists the names in a directory
/// or `nil` and an error message.
fn list(dir: String) -> Result<(Option<Listing>, Option<String>)> {
    match std::fs::read_dir(if dir.is_empty() { "." } else { &dir }) {
        Ok(ls) => Ok((Some(Listing::new(ls)), None)),
        Err(err) => Ok((None, Some(err.to_string()))),
    }
}

/// Exported in debug.
/// Pretty-print a value to string.
fn pretty(obj: Value) -> Result<String> {
    Ok(format!("{obj:#?}"))
}

/// Exported in string.
/// Split a line of input into lua tokens.
fn prompt_lua_tokens_split(line: String, point: Option<usize>) -> Result<PromptSplitInfo> {
    Ok(prompt::lua_tokens_split(&line, point))
}

/// Exported in string.
/// Split a line of input in a shell-like manner.
fn prompt_shell_like_split(line: String, point: Option<usize>) -> Result<PromptSplitInfo> {
    Ok(prompt::shell_like_split(&line, point))
}

fn transpose_keytranserror(seq: &str, err: KeyTransError) -> Error {
    match err {
        KeyTransError::UnfinishedForm(start) => Error::SyntaxError {
            message: format!(
                "unfinished key starting at character {start}: {:?}",
                // xxx: yea this will break some utf8 chars...
                &seq[std::cmp::max(4, start) - 4..std::cmp::min(seq.len() - 1, start + 4)],
            ),
            incomplete_input: true,
        },
        KeyTransError::UnknownForm(slice) => Error::SyntaxError {
            message: format!("unknown key {slice:?} in: {seq:?}"),
            incomplete_input: false,
        },
    }
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
