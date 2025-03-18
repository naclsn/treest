use std::io::{self, Read};
use std::path::PathBuf;

use mlua::{Function, Lua, Result as LuaResult, UserData, UserDataFields, UserDataMethods, Value};

use crate::navigate::{Navigate, Target, ViewJumpBy};
use crate::prompt::{self, PromptSplitInfo};
use crate::terminal;
use crate::tree::NodePath;

impl UserData for Navigate {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("quitting", |_, nav| Ok(nav.exit.is_some()));
        fields.add_field_method_get("mouse_event_pos", |_, nav| {
            Ok(nav.input.get_pending_mouse_info())
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method_mut("_atexit", |_, nav, ()| nav._atexit());
        methods.add_method_mut("_tick", |_, nav, ()| nav._tick());
        methods.add_method_mut("map", |_, nav, (seq, cb)| nav.map(seq, cb));
        methods.add_method_mut("message", |_, nav, text| nav.message(text));
        methods.add_method_mut("quit", |_, nav, text| nav.quit(text));
        methods.add_method_mut("unfold", |_, nav, target| nav.unfold(target));
    }
}

pub fn other_exports(lua: &Lua) -> LuaResult<()> {
    let g = lua.globals();
    g.raw_set("help", lua.create_function(|_, subj| help(subj))?)?;
    Ok(())
}

impl Navigate {
    fn _atexit(&mut self) -> LuaResult<String> {
        let exit = self
            .exit
            .take()
            .expect("_atexit called too early (no exit text set)");
        self.term.take().map(|t| t.restore());
        Ok(exit)
    }

    fn _tick(&mut self) -> LuaResult<Option<Function>> {
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

    fn get_option(&mut self, lua: &Lua, name: String) -> LuaResult<Value> {
        Ok(self.options.get(&name, lua))
    }

    // TODO: remove the Option<>
    fn get_register(&mut self, name: String) -> LuaResult<Option<String>> {
        Ok(self.registers.get(&name).and_then(|h| h.last()).cloned())
    }

    // TODO: remove the Option<>
    fn get_register_hist(&mut self, name: String) -> LuaResult<Option<&[String]>> {
        Ok(self.registers.get(&name).map(|h| &h[..]))
    }

    fn map(&mut self, seq: String, cb: Function) -> LuaResult<()> {
        let seq = terminal::keytrans(seq.as_str()).expect("need valid seq something blbl TODO");
        self.input.add_mapping(seq, cb);
        Ok(())
    }

    fn message(&mut self, text: Option<String>) -> LuaResult<()> {
        self.message = text.map(|w| w.replace("\n", "\r\n")); // TODO: somewhat of a temp hack
        Ok(())
    }

    fn prompt(&mut self, ps: String, completion: Function) -> LuaResult<Option<String>> {
        let history = self.registers.entry(ps.clone()).or_default();

        terminal::cursor_on();
        terminal::mouse_off();
        let ans = prompt::prompt(
            &ps,
            io::stdin().bytes().map_while(Result::ok),
            io::stderr(),
            history.clone(),
            |line, point| completion.call((line, point)).unwrap_or_default(),
        );
        terminal::cursor_off();
        terminal::mouse_on();

        ans.as_ref().inspect(|r| history.push(r.to_string()));
        Ok(ans)
    }

    fn provider_name(&self) -> LuaResult<String> {
        Ok(self.provider_name.clone())
    }

    fn quit(&mut self, text: Option<String>) -> LuaResult<()> {
        self.exit = text.or(Some(String::new()));
        Ok(())
    }

    fn set_option(&mut self, name: String, value: Value) -> LuaResult<()> {
        self.options.set(&name, value);
        Ok(())
    }

    fn set_register(&mut self, name: String) -> LuaResult<Option<String>> {
        Ok(self.registers.get(&name).and_then(|h| h.last()).cloned())
    }

    fn suspend(&mut self) -> LuaResult<()> {
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

    fn unfold(&mut self, target: Option<Vec<usize>>) -> LuaResult<()> {
        self.set_folded(
            target
                .as_deref()
                .map(Target::Path)
                .unwrap_or(Target::Cursor),
            false,
        );
        Ok(())
    }
}

fn help(subj: String) -> LuaResult<Option<String>> {
    Ok("idk".to_string().into()) // TODO ofc
}

fn keyseqstr(seq: Vec<u8>) -> LuaResult<String> {
    Ok(terminal::keyseqstr(&seq))
}

fn keytrans(text: &str) -> LuaResult<Option<Vec<u8>>> {
    Ok(terminal::keytrans(text))
}

fn prompt(ps: String, history: Vec<String>, completion: Function) -> LuaResult<Option<String>> {
    terminal::cursor_on();
    terminal::mouse_off();
    let ans = prompt::prompt(
        &ps,
        io::stdin().bytes().map_while(Result::ok),
        io::stderr(),
        history.clone(),
        |line, point| completion.call((line, point)).unwrap_or_default(),
    );
    terminal::cursor_off();
    terminal::mouse_on();
    Ok(ans)
}

// TODO: maybe attach to string table
fn prompt_split(line: String, point: Option<usize>) -> LuaResult<PromptSplitInfo> {
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

/*
#[export_module]
mod api {
    // node actions {{{

    #[rhai_fn(return_raw)]
    pub fn fold(api: &mut Api, is: bool, path: Array) -> ApiResult<()> {
        let path = host_path(&path)?;
        api.m().set_folded(Target::Path(&path), is);
        Ok(())
    }
    #[rhai_fn(name = "fold")]
    pub fn fold_cursor(api: &mut Api, is: bool) {
        api.m().set_folded(Target::Cursor, is);
    }
    #[rhai_fn(return_raw)]
    pub fn folded(api: &mut Api, path: Array) -> ApiResult<bool> {
        let path = host_path(&path)?;
        Ok(api.m().get_folded(Target::Path(&path)))
    }
    #[rhai_fn(name = "folded")]
    pub fn folded_cursor(api: &mut Api) -> bool {
        api.m().get_folded(Target::Cursor)
    }

    #[rhai_fn(return_raw)]
    pub fn mark(api: &mut Api, is: bool, path: Array) -> ApiResult<()> {
        let path = host_path(&path)?;
        api.m().set_marked(Target::Path(&path), is);
        Ok(())
    }
    #[rhai_fn(name = "mark")]
    pub fn mark_cursor(api: &mut Api, is: bool) {
        api.m().set_marked(Target::Cursor, is);
    }
    #[rhai_fn(return_raw)]
    pub fn marked(api: &mut Api, path: Array) -> ApiResult<bool> {
        let path = host_path(&path)?;
        Ok(api.m().get_marked(Target::Path(&path)))
    }
    #[rhai_fn(name = "marked")]
    pub fn marked_cursor(api: &mut Api) -> bool {
        api.m().get_marked(Target::Cursor)
    }

    // }}}

    // cursor movement {{{

    pub fn enter(api: &mut Api) {
        api.m().enter();
    }
    pub fn leave(api: &mut Api) {
        api.m().leave();
    }

    pub fn next(api: &mut Api, wrapping: &str) {
        api.m().next("wrap" == wrapping);
    }
    pub fn prev(api: &mut Api, wrapping: &str) {
        api.m().prev("wrap" == wrapping);
    }
    #[rhai_fn(name = "next")]
    pub fn next_sat(api: &mut Api) {
        api.m().next(false);
    }
    #[rhai_fn(name = "prev")]
    pub fn prev_sat(api: &mut Api) {
        api.m().prev(false);
    }

    #[rhai_fn(return_raw)]
    pub fn jumpto(api: &mut Api, path: Array) -> ApiResult<()> {
        // TODO: check validity before assigning, unfolding and cropping as needed
        let path = host_path(&path)?;
        api.m().cursor = (path.len(), path);
        Ok(())
    }
    #[rhai_fn(pure)]
    pub fn cursor(api: &mut Api) -> Array {
        script_path(api.m().cursor())
    }

    // }}}

    // view {{{

    pub fn view_down(api: &mut Api, by: &str) {
        api.m().view.borrow_mut().down(match by {
            "win" => ViewJumpBy::Win,
            "halfwin" => ViewJumpBy::HalfWin,
            "mouse" => ViewJumpBy::Mouse,
            _ => ViewJumpBy::Line,
        });
    }
    pub fn view_up(api: &mut Api, by: &str) {
        api.m().view.borrow_mut().up(match by {
            "win" => ViewJumpBy::Win,
            "halfwin" => ViewJumpBy::HalfWin,
            "mouse" => ViewJumpBy::Mouse,
            _ => ViewJumpBy::Line,
        });
    }
    #[rhai_fn(name = "view_down")]
    pub fn view_down_line(api: &mut Api) {
        api.m().view.borrow_mut().down(ViewJumpBy::Line);
    }
    #[rhai_fn(name = "view_up")]
    pub fn view_up_line(api: &mut Api) {
        api.m().view.borrow_mut().up(ViewJumpBy::Line);
    }

    // }}}

    // searches {{{
    // xxx: searches are cursor-centered...

    /// Search for a matching node at the cursor level (ie. siblings).
    ///
    /// Search starts at cursor index, `next_prev` should be `"next"` or `"prev"`
    /// to search forward or backward respectively. `wrapping` should be `"wrap"`
    /// or `"sat"` to indicate whether search should wrap around.
    ///
    /// Uses the `api["/"]` register.
    pub fn search_level(api: &mut Api, next_prev: &str, wrapping: &str) -> Dynamic {
        with_opt_unit(|| {
            let nav = api.0.borrow();

            if nav.is_cursor_root() {
                return None::<Vec<_>>;
            };
            let s = nav.registers.get("/").and_then(|v| v.last())?;
            let [parent_path @ .., current] = nav.cursor() else {
                unreachable!();
            };
            let parent = nav.tree.resolve(parent_path);
            let chs = parent.last().unwrap().children().unwrap();

            let found = slice_search(
                &chs,
                *current,
                |node| {
                    nav.provider
                        .display(&NodePath {
                            head: &parent,
                            tail: node,
                        })
                        .contains(s)
                },
                "next" == next_prev,
                "wrap" == wrapping,
            )?;

            let mut r = script_path(parent_path);
            r.push((found as INT).into());
            Some(r)
        })
    }

    pub fn search_next(api: &mut Api, q: &str, wrapping: &str) -> Dynamic {
        api.m().register_push("/".into(), q.into());
        search_next_already(api, wrapping)
    }
    #[rhai_fn(name = "search_next")]
    pub fn search_next_already(api: &mut Api, wrapping: &str) -> Dynamic {
        search_level(api, "next", wrapping)
    }
    #[rhai_fn(name = "search_next")]
    pub fn search_next_already_sat(api: &mut Api) -> Dynamic {
        search_next_already(api, "sat")
    }

    pub fn search_prev(api: &mut Api, q: &str, wrapping: &str) -> Dynamic {
        api.m().register_push("/".into(), q.into());
        search_prev_already(api, wrapping)
    }
    #[rhai_fn(name = "search_prev")]
    pub fn search_prev_already(api: &mut Api, wrapping: &str) -> Dynamic {
        search_level(api, "prev", wrapping)
    }
    #[rhai_fn(name = "search_prev")]
    pub fn search_prev_already_sat(api: &mut Api) -> Dynamic {
        search_prev_already(api, "sat")
    }

    pub fn search_deep(api: &mut Api, q: &str, wrapping: &str) -> Dynamic {
        api.m().register_push("/".into(), q.into());
        search_deep_already(api, wrapping)
    }
    #[rhai_fn(name = "search_deep")]
    pub fn search_deep_already(api: &mut Api, wrapping: &str) -> Dynamic {
        with_opt_unit(|| None::<Vec<Dynamic>>)
    }
    #[rhai_fn(name = "search_deep")]
    pub fn search_deep_already_sat(api: &mut Api) -> Dynamic {
        search_deep_already(api, "sat")
    }

    // }}}
}
*/

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
