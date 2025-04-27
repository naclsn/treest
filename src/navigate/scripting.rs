use std::fs::File;
use std::io::{self, Write};

use mlua::{AnyUserData, MetaMethod, UserData, UserDataFields, UserDataMethods};
use mlua::{BString, Either, Error, Function, Lua, Result, Table, Value};

use crate::lua::help;
use crate::lua::structs::{Completion, MappingFn, PromptAnsCallback};
use crate::lua::structs::{IndexPath, Listing, NodeInfo, PromptSplitInfo, Target};
use crate::lua::structs::{MoveFlags, ProviderFlags, ScrollFlags, SearchFlags};
use crate::navigate::input::InputTickResponse;
use crate::navigate::options::GlobalOptions;
use crate::navigate::{Navigate, ViewJumpBy};
use crate::prompt::{self, Prompt};
use crate::provider::{self, Event, EventKind};
use crate::terminal::{self, KeyTransError};
use crate::tree::NodePath;

macro_rules! make_exports {
    ($t:expr, $lua:ident; $(fn $name:ident($($param:ident),*);)*) => {
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
    ($methods:ident; $(fn $name:ident($($param:ident),*) $($mut:ident)?;)*) => {
        $(make_methods!(@ $methods; fn $name($($param),*) $($mut)?);)*

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

    (@ $methods:ident; fn $name:ident($($param:ident),*) mut) => {
        $methods.add_method_mut(stringify!($name), |_, this, ($($param,)*)| this.$name($($param),*))
    };
    (@ $methods:ident; fn $name:ident($($param:ident),*)) => {
        $methods.add_method(stringify!($name), |_, this, ($($param,)*)| this.$name($($param),*))
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
        methods.add_method_mut("_atexit", |_, this, ()| this._atexit());
        methods.add_method_mut("_tick", |_, this, ()| this._tick());

        make_methods! { methods;
            //fn provider_request(req, target, text) mut;
            fn cursor_get();
            fn cursor_set(target) mut;
            fn force_redraw() mut;
            fn key_map(seq, cb) mut;
            fn key_mapped(seq);
            fn key_raw(keys) mut;
            fn key_unmap(seq) mut;
            fn message(text) mut;
            fn message_scroll_down(by) mut;
            fn message_scroll_up(by) mut;
            fn node_enter() mut;
            fn node_fold(target) mut;
            fn node_folded(target);
            fn node_info(target);
            fn node_info_at_line(line);
            fn node_leave() mut;
            fn node_mark(target) mut;
            fn node_marked(target);
            fn node_next(flags) mut;
            fn node_prev(flags) mut;
            fn node_unfold(target) mut;
            fn node_unmark(target) mut;
            fn option_get(name);
            fn option_list();
            fn option_set(name, value) mut;
            fn provider_join_components(components);
            fn provider_name();
            fn quit(text) mut;
            fn register_get(name);
            fn register_get_hist(name);
            fn register_list();
            fn register_prompt(ps, completion, then) mut;
            fn register_set(name, value) mut;
            fn request_create(target, text) mut;
            fn request_remove(target) mut;
            fn search_deep(q, flags) mut;
            fn search_level(q, flags);
            fn space_close(placement) mut;
            fn space_count();
            fn space_current();
            fn space_guess(arg);
            fn space_next(flags) mut;
            fn space_open(arg, name, placement_hint) mut;
            fn space_prev(flags) mut;
            fn space_replace(arg, name, placement) mut;
            fn space_swap(with, placement) mut;
            fn suspend() mut;
            fn view_down(by) mut;
            fn view_up(by) mut;
        }
    }
}

pub fn global_exports(g: &Table, lua: &Lua) -> Result<()> {
    make_exports! { g, lua;
        fn help(subj);
    }
    make_exports! { g.get("os").unwrap(), lua;
        fn list(dir);
    }
    make_exports! { g.get("string").unwrap(), lua;
        fn key_seqstr(seq);
        fn key_trans(text);
        fn prompt_lua_tokens_split(line, point);
        fn prompt_shell_like_split(line, point);
    }
    make_exports! { g.get("debug").unwrap(), lua;
        fn pretty(obj);
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
        // no redraw if there are still keys that can be processed
        if !self.input.has_recycle() || self.force_redraw {
            let mut buf = b"\x1b7".to_vec();
            if self.force_redraw {
                buf.extend(b"\x1b[2J");
            }
            self.render(&mut buf, self.force_redraw)?;
            buf.extend(b"\x1b8");
            if let Err(err) = io::stderr().write_all(&buf) {
                // assume unrecoverable situation, bail out
                self.exit = Some(err.to_string());
                return Ok(None);
            }
        }
        self.force_redraw = false;

        Ok(match self.input.tick() {
            InputTickResponse::Noop => None,
            InputTickResponse::EndOfInput => {
                // assume unrecoverable situation, bail out
                self.exit = Some("end of input".to_string());
                None
            }

            // rem: functions cannot be called here beacause `self` is borrowed mutably
            // (it would cause a BadArgument: UserDataBorrowMutError)
            InputTickResponse::CallbackMapping(action) => Some(action.into()),
            InputTickResponse::CallbackPrompt { ps, callback, ans } => {
                let sps = &ps[..ps.bytes().position(|c| b'\x1b' == c).unwrap_or(ps.len())];
                self.register_push(sps, ans.clone());
                Some(callback.bind_all(ans)?)
            }
        })
    }

    /// Exported in treest.
    /// Retrieve the cursor path.
    ///
    /// Using this where a `Target` is expected is equivalent to `nil`.
    fn cursor_get(&self) -> Result<Vec<usize>> {
        Ok(self.space().cursor().to_vec())
    }

    /// Exported in treest.
    /// Move the cursor the the given target.
    ///
    /// If the target is not valid, the longest valid path is used (for now we just crash).
    fn cursor_set(&mut self, target: Target) -> Result<()> {
        if let Target::Path(path) = target {
            self.space_mut().cursor_to(path.len(), path);
        }
        Ok(())
    }

    /// Exported in treest.
    /// Force a redraw.
    ///
    /// The next draw will be a full redraw instead of only drawing what is necessary.
    fn force_redraw(&mut self) -> Result<()> {
        self.force_redraw = true;
        Ok(())
    }

    /// Exported in treest.
    /// Add a mapping from a key sequence to a callback action.
    ///
    /// See also `treest:key_unmap`.
    fn key_map(&mut self, seq: String, cb: MappingFn) -> Result<()> {
        let seq = terminal::keytrans(&seq).map_err(|err| transpose_keytranserror(&seq, err))?;
        self.input.add_mapping(seq, cb);
        Ok(())
    }

    /// Exported in treest.
    /// Retrieve a mapping from a key sequence, returning its action callback.
    ///
    /// Result will be `nil` if `seq` wasn't mapped (see `treest:key_map`).
    fn key_mapped(&self, seq: String) -> Result<Option<MappingFn>> {
        let seq = terminal::keytrans(&seq).map_err(|err| transpose_keytranserror(&seq, err))?;
        Ok(self.input.get_mapping(seq).cloned())
    }

    /// Exported in treest.
    /// Send a raw sequence of keys as if typed.
    ///
    /// This is rarely the surest and most efficient way to go about it,
    /// but certainly the easiest and most practical.
    fn key_raw(&mut self, seq: String) -> Result<()> {
        let seq = terminal::keytrans(&seq).map_err(|err| transpose_keytranserror(&seq, err))?;
        self.input.recycle(seq);
        Ok(())
    }

    /// Exported in treest.
    /// Remove a mapping from a key sequence, returning its previously associated action callback.
    ///
    /// Result will be `nil` if `seq` wasn't mapped (see `treest:key_mapped`).
    fn key_unmap(&mut self, seq: String) -> Result<Option<MappingFn>> {
        let seq = terminal::keytrans(&seq).map_err(|err| transpose_keytranserror(&seq, err))?;
        Ok(self.input.pop_mapping(seq))
    }

    /// Exported in treest.
    /// Set the message text.
    ///
    /// A multiline message can be interacted with through `treest:message_scroll_up`
    /// and `treest:message_scroll_down`. Setting to an empty vector will essentially clear it.
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
        let msh = self.options.make_ref().lock().messageheight as usize;
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
            self.options.make_ref().lock().messageheight as usize,
            0,
        );
        Ok(())
    }

    /// Exported in treest.
    /// Try to enter the node at cursor (ie relative motion), unfolding it as needed.
    ///
    /// Nothing happens if it cannot be unfolded.
    fn node_enter(&mut self) -> Result<()> {
        self.space_mut().cursor_enter();
        Ok(())
    }

    /// Exported in treest.
    /// Fold the node at target (cursor if `nil`).
    ///
    /// Nothing happens if the target is not valid.
    fn node_fold(&mut self, target: Target) -> Result<()> {
        self.space_mut().set_folded(target, true);
        Ok(())
    }

    /// Exported in treest.
    /// Check if the node at target (cursor if `nil`) is folded.
    ///
    /// Return `nil` if the target is not valid.
    fn node_folded(&self, target: Target) -> Result<Option<bool>> {
        Ok(self.space().get_folded(target))
    }

    /// Exported in treest.
    /// Retrieve node information at target (cursor if `nil`) or `nil` if the path is not valid.
    fn node_info(&self, target: Target) -> Result<Option<NodeInfo>> {
        Ok(self.space().retrieve_node_info(target))
    }

    /// Exported in treest.
    /// Retrieve node information at a given display line (or 'row'), or `nil` if there is none.
    fn node_info_at_line(&self, line: usize) -> Result<Option<NodeInfo>> {
        let space = self.space();
        Ok(space
            .view
            .path_for(line)
            .and_then(|path| space.retrieve_node_info(Target::TrustedPath(path.clone()))))
    }

    /// Exported in treest.
    /// Try to leave the node at cursor (ie relative motion).
    ///
    /// Nothing happens if the cursor is already at root node.
    fn node_leave(&mut self) -> Result<()> {
        self.space_mut().cursor_leave();
        Ok(())
    }

    /// Exported in treest.
    /// Mark the node at target (cursor if `nil`).
    ///
    /// Nothing happens if the target is not valid.
    fn node_mark(&mut self, target: Target) -> Result<()> {
        self.space_mut().set_marked(target, true);
        Ok(())
    }

    /// Exported in treest.
    /// Check if the node at target (cursor if `nil`) is marked.
    ///
    /// Return `nil` if the target is not valid.
    fn node_marked(&self, target: Target) -> Result<Option<bool>> {
        Ok(self.space().get_marked(target))
    }

    /// Exported in treest.
    /// Move cursor to the next sibling node (ie relative motion).
    ///
    /// When flag is 'sat' and it's the last child, nothing happens.
    /// 'wrap' will instead go back to first child.
    fn node_next(&mut self, flags: MoveFlags) -> Result<()> {
        self.space_mut().cursor_next("wrap" == flags.wrapping);
        Ok(())
    }

    /// Exported in treest.
    /// Move cursor to the previous sibling node (ie relative motion).
    ///
    /// When flag is 'sat' and it's the first child, nothing happens.
    /// 'wrap' will instead go back to last child.
    fn node_prev(&mut self, flags: MoveFlags) -> Result<()> {
        self.space_mut().cursor_prev("wrap" == flags.wrapping);
        Ok(())
    }

    /// Exported in treest.
    /// Unfold the node at target (cursor if `nil`).
    fn node_unfold(&mut self, target: Target) -> Result<()> {
        self.space_mut().set_folded(target, false);
        Ok(())
    }

    /// Exported in treest.
    /// Unmark the node at target (cursor if `nil`).
    fn node_unmark(&mut self, target: Target) -> Result<()> {
        self.space_mut().set_marked(target, false);
        Ok(())
    }

    /// Exported in treest.
    /// Get the value of an option.
    ///
    /// See `treest:option_list` for a list of the available option names.
    fn option_get(&self, name: String) -> Result<Either<String, Either<isize, bool>>> {
        self.options
            .make_ref()
            .lock()
            .get(&name)
            .ok_or(Error::BadArgument {
                to: Some("option_get".to_string()),
                pos: 2,
                name: Some("name".to_string()),
                cause: Error::RuntimeError(format!("no option '{name:?}'")).into(),
            })
    }

    /// Exported in treest.
    /// List the available options (*names* only).
    fn option_list(&self) -> Result<Vec<String>> {
        Ok(GlobalOptions::list()
            .iter()
            .map(|s| s.to_string())
            .collect())
    }

    /// Exported in treest.
    /// Set the value of an option.
    ///
    /// See `treest:option_list` for a list of the available option names.
    fn option_set(
        &mut self,
        name: String,
        value: Either<String, Either<isize, bool>>,
    ) -> Result<()> {
        self.options
            .lock_mut()
            .set(&name, value)
            .ok_or(Error::BadArgument {
                to: Some("option_set".to_string()),
                pos: 2,
                name: Some("name".to_string()),
                cause: Error::RuntimeError(format!("no option '{name:?}'")).into(),
            })
    }

    /// Exported in treest.
    /// Let the provider join the given components.
    ///
    /// Components may be obtained with `treest:node_info`.
    fn provider_join_components(&self, components: Vec<String>) -> Result<String> {
        Ok(self.space().provider.join(&components))
    }

    /// Exported in treest.
    /// Retrieve the name of the current provider (eg. 'fs').
    fn provider_name(&self) -> Result<String> {
        Ok(self.space().provider_name().to_string())
    }

    /// Exported in treest.
    /// Quit the application.
    ///
    /// If the text is non-empty, it is considered an error message to be printed and the exit code will be 1.
    /// Only `nil` is considered a normal exit situation.
    ///
    /// To merely close the focused space, see `treest:space_close`.
    fn quit(&mut self, text: Option<String>) -> Result<()> {
        self.exit = text.or(Some(String::new()));
        Ok(())
    }

    /// Exported in treest.
    /// Get the value of a register or nil if it doesn't exist.
    fn register_get(&self, name: String) -> Result<Option<String>> {
        Ok(self.registers.get(&name).and_then(|h| h.last()).cloned())
    }

    /// Exported in treest.
    /// Get the values taken by a register or nil if it doesn't exist.
    ///
    /// This includes the current value which will be the last entry.
    fn register_get_hist(&self, name: String) -> Result<Option<Vec<String>>> {
        Ok(self.registers.get(&name).cloned())
    }

    /// Exported in treest.
    /// List the non-empty registers (*names* only).
    fn register_list(&self) -> Result<Vec<String>> {
        Ok(self.registers.keys().cloned().collect())
    }

    /// Exported in treest.
    /// Prompt the user for a line of input.
    ///
    /// The result is stored in the register given by `ps` as well as given
    /// in argument to the callback function. If the prompt was discarded
    /// then the callback is not called and registers are not updated.
    ///
    /// History is also taken from the previous values of the register.
    /// The `point` argument to the `completion` function is 0-base.
    ///
    /// See also `treest:register_set` for direct register access.
    ///
    /// There is a special case for the handling of `ps` as a register name:
    /// if `ps` contains the byte 0x1b (escape), only the text before it
    /// is used in setting the register. The whole string will still be used
    /// for the actual prompt.
    fn register_prompt(
        &mut self,
        ps: String,
        completion: Completion,
        callback: Option<PromptAnsCallback>,
    ) -> Result<()> {
        terminal::cursor(true); // reset again in Input::tick
        eprint!("{ps}");
        let sps = &ps[..ps.bytes().position(|c| b'\x1b' == c).unwrap_or(ps.len())];
        let history = self.register_entries(sps).to_vec();
        self.input.set_prompt(
            Prompt::new(
                ps,
                history,
                Box::new(move |line: &str, point| {
                    completion
                        .call(line, point)
                        .unwrap_or_default()
                        .unwrap_or_default()
                }),
            ),
            callback,
        );
        Ok(())
    }

    /// Exported in treest.
    /// Set the value of a register.
    ///
    /// Registers are also set when using `treest:register_prompt`.
    ///
    /// The `name` is always trimmed of leading and trailing whitespaces.
    fn register_set(&mut self, name: String, value: String) -> Result<()> {
        self.register_push(&name, value);
        Ok(())
    }

    /// Exported in treest.
    /// Request the provider for a create event.
    ///
    /// `target` may be nil for cursor. `text` interpretation may be provider-dependent.
    fn request_create(&mut self, target: Target, text: String) -> Result<()> {
        self.space_mut()
            .request_event_create(target, text)
            .map_err(Error::external)
    }

    /// Exported in treest.
    /// Request the provider for a remove event.
    ///
    /// `target` may be nil for cursor.
    fn request_remove(&mut self, target: Target) -> Result<()> {
        self.space_mut()
            .request_event_remove(target)
            .map_err(Error::external)
    }

    /// Exported in treest.
    /// Search for a node with `q` in its text.
    ///
    /// Not implemented yet.
    /// The search is performed depth-first, greedily unfolding nodes as needed.
    fn search_deep(&mut self, _q: String, _flags: SearchFlags) -> Result<Option<Vec<usize>>> {
        todo!()
    }

    /// Exported in treest.
    /// Search for a sibling node with `q` in its text.
    fn search_level(&self, q: String, flags: SearchFlags) -> Result<Option<IndexPath>> {
        let space = self.space();
        if space.is_cursor_root() {
            return Ok(None);
        };
        let [parent_path @ .., current] = space.cursor() else {
            unreachable!();
        };
        let parent = space.tree.resolve(parent_path);
        let children: Vec<_> = parent.last().unwrap().children().unwrap().collect();

        let Some(found) = slice_search(
            &children,
            *current,
            |node| {
                space
                    .provider
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
    /// Close the space at `placement` (1-base, current if `nil`).
    fn space_close(&mut self, placement: Option<usize>) -> Result<()> {
        let at = placement.unwrap_or(self.current_space + 1) - 1;
        self.remove_space(at);
        Ok(())
    }

    /// Exported in treest.
    /// Current number of open spaces.
    fn space_count(&self) -> Result<usize> {
        Ok(self.spaces.len())
    }

    /// Exported in treest.
    /// Current space index (1-base).
    fn space_current(&self) -> Result<usize> {
        Ok(self.current_space + 1)
    }

    /// Exported in treest.
    /// Try to guess the name of a provider for `arg`.
    fn space_guess(&self, arg: String) -> Result<Option<String>> {
        Ok(provider::guess(&arg).map(String::from))
    }

    /// Exported in treest.
    /// Move focus to the next (right) space.
    ///
    /// When flag is 'sat' and it's the rightmost space, nothing happens.
    /// 'wrap' will instead go back to leftmost space.
    fn space_next(&mut self, flags: MoveFlags) -> Result<()> {
        self.current_space = match flags.wrapping {
            "wrap" => (self.current_space + 1) % self.spaces.len(),
            "sat" => std::cmp::max(self.current_space + 1, self.spaces.len() - 1),
            _ => unreachable!(),
        };
        Ok(())
    }

    /// Exported in treest.
    /// Open a new space given `arg`.
    ///
    /// `name` may be needed if the provider cannot be guessed
    /// (see `treest:space_guess` for this).
    ///
    /// `placement_hint` is a 1-base index of where to place the new
    /// space. By default it will be the current space's placement.
    /// The new space will be placed after (ie to the right of).
    /// 0 can be used to place the new space left-most.
    fn space_open(
        &mut self,
        arg: String,
        name: Option<ProviderFlags>,
        placement_hint: Option<usize>,
    ) -> Result<()> {
        let provider_name = match name {
            Some(ProviderFlags { name }) => name,
            None => provider::guess(&arg).ok_or(Error::BadArgument {
                to: Some("space_open".to_string()),
                pos: 3,
                name: Some("name".to_string()),
                cause: Error::RuntimeError("provider could not be guessed, name is needed".into())
                    .into(),
            })?,
        };
        let provider = provider::select(&arg, provider_name).map_err(Error::external)?;
        let at = placement_hint.unwrap_or(self.current_space);
        self.insert_space(at, provider, provider_name.to_string());
        Ok(())
    }

    /// Exported in treest.
    /// Move focus to the previous (left) space.
    ///
    /// When flag is 'sat' and it's the leftmost space, nothing happens.
    /// 'wrap' will instead go back to rightmost space.
    fn space_prev(&mut self, flags: MoveFlags) -> Result<()> {
        self.current_space = match flags.wrapping {
            "wrap" => (self.current_space + self.spaces.len() - 1) % self.spaces.len(),
            "sat" => self.current_space.saturating_sub(1),
            _ => unreachable!(),
        };
        Ok(())
    }

    /// Exported in treest.
    /// Open a new space and replace the existing one at `placement` (or current if `nil`).
    ///
    /// `name` may be needed if the provider cannot be guessed
    /// (see `treest:space_guess` for this).
    ///
    /// This is somewhat equivalent to using bot `treest:space_close` and `treest:space_open`.
    fn space_replace(
        &mut self,
        arg: String,
        name: Option<ProviderFlags>,
        placement: Option<usize>,
    ) -> Result<()> {
        let provider_name = match name {
            Some(ProviderFlags { name }) => name,
            None => provider::guess(&arg).ok_or(Error::BadArgument {
                to: Some("space_replace".to_string()),
                pos: 3,
                name: Some("name".to_string()),
                cause: Error::RuntimeError("provider could not be guessed, name is needed".into())
                    .into(),
            })?,
        };
        let provider = provider::select(&arg, provider_name).map_err(Error::external)?;
        let at = placement.unwrap_or(self.current_space);
        self.replace_space(at, provider, provider_name.to_string());
        Ok(())
    }

    /// Exported in treest.
    /// Swap with an other spaces.
    ///
    /// If both arguments `with` and `placement` are provided, swap these instead of current.
    /// All are 1-base.
    fn space_swap(&mut self, with: usize, placement: Option<usize>) -> Result<()> {
        let at = placement.unwrap_or(self.current_space + 1) - 1;
        self.swap_spaces(at, with);
        Ok(())
    }

    /// Exported in treest.
    /// Suspend execution for job control (by raising a SIGTSTP).
    ///
    /// This is like hitting `<C-Z>` on most terminal programs.
    /// It is a no-op under Windows.
    fn suspend(&mut self) -> Result<()> {
        #[cfg(not(windows))]
        {
            let options = self.options.make_ref();
            let (mouse, altscreen) = {
                let o = options.lock();
                (o.mouse, o.altscreen)
            };

            terminal::cursor(true);
            if mouse {
                terminal::mouse(false);
            }
            if altscreen {
                terminal::altscreen(false);
            }

            if let Some(t) = self.term.take() {
                t.restore();
            }
            unsafe { libc::raise(libc::SIGTSTP) };
            self.term = terminal::raw_with_panic_hook().ok();
            self.force_redraw = true;

            // sanely assume these options weren't changed in the mean time
            terminal::cursor(false);
            if mouse {
                terminal::mouse(true);
            }
            if altscreen {
                terminal::altscreen(true);
            }
        }
        Ok(())
    }

    /// Exported in treest.
    /// Move the view down, revealing any hidden lines at the bottom.
    fn view_down(&mut self, by: ScrollFlags) -> Result<()> {
        let mut space = self.space_mut();
        ViewJumpBy::new_by(by.amount).view_down(&mut space.view);
        Ok(())
    }

    /// Exported in treest.
    /// Move the view up, revealing any hidden lines at the top.
    fn view_up(&mut self, by: ScrollFlags) -> Result<()> {
        let mut space = self.space_mut();
        ViewJumpBy::new_by(by.amount).view_up(&mut space.view);
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
            let options = GlobalOptions::list().iter().map(|op| format!("'{op}'"));
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

        o if o.starts_with("'") => GlobalOptions::help(&o.strip_suffix("'").unwrap_or(o)[1..])
            .map(|ln| ln.join("\n") + "\n"),

        _ => None,
    })
}

/// Exported in string.
/// Translate a byte string back to a key sequence: `str:key_seqstr():key_trans() == str`.
fn key_seqstr(bytes: BString) -> Result<String> {
    Ok(terminal::keyseqstr(&bytes))
}

/// Exported in string.
/// Translate a key sequence into the corresponding byte string.
///
/// Note that `treest:key_map` expects a non-translated string! (Tho the result will be the same.)
///
/// Notations are mostly taken from Vim. Here are the recognised forms:
/// ```text
/// <Nul> <BS> <Tab> <NL> <CR> <Space> <lt> <gt> <Bslash> <Bar> <CSI>
/// <Up> <Down> <Right> <Left>
/// <Home> <End> <Insert> <Delete> <PageUp> <PageDown>
/// <LeftMouse> <RightMouse> <ForwardWheel> <BackwardWheel> <UpMouse>
/// <C-..> <M-..> <A-..>
/// ```
fn key_trans(seq: String) -> Result<BString> {
    terminal::keytrans(&seq)
        .map_err(|err| transpose_keytranserror(&seq, err))
        .map(BString::from)
}

/// Exported in os.
/// Return an iterator function that lists the names in a directory or `nil` and an error message.
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
        (true, false) => 1..slice.len() - from,
        (false, false) => 1..from + 1,
    }
    .map(|k| (from + k * dir) % slice.len())
    .find(|n| predicate(&slice[*n]))
}

#[cfg(test)]
mod test {
    #[test]
    fn slice_search() {
        macro_rules! assert_slice_search {
            ($sl:expr, $at:expr, $it:expr, $fw:expr, $wp:expr, $xp:expr) => {
                assert_eq!(super::slice_search($sl, $at, |c| $it == *c, $fw, $wp), $xp);
            };
        }
        assert_slice_search!(b"abcdefg", 4, b'f', true, false, Some(5));
        assert_slice_search!(b"abcdefg", 4, b'b', true, false, None);
        assert_slice_search!(b"abcdefg", 4, b'b', true, true, Some(1));
        assert_slice_search!(b"abcdefg", 4, b'e', true, false, None);
        assert_slice_search!(b"abcdefg", 4, b'e', true, true, None);
        assert_slice_search!(b"abcdefg", 4, b'g', true, false, Some(6));
        assert_slice_search!(b"abcdefg", 4, b'c', false, false, Some(2));
        assert_slice_search!(b"abcdefg", 4, b'g', false, false, None);
        assert_slice_search!(b"abcdefg", 4, b'g', false, true, Some(6));
        assert_slice_search!(b"abcdefg", 4, b'e', false, false, None);
        assert_slice_search!(b"abcdefg", 4, b'e', false, true, None);
        assert_slice_search!(b"ooxxoxoxx", 4, b'o', false, false, Some(1));
        assert_slice_search!(b"ooxxoxoxx", 1, b'o', false, false, Some(0));
    }
}
