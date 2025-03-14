use std::cell::{RefCell, RefMut};
use std::io::{self, Read};
use std::path::PathBuf;
use std::rc::Rc;

use rhai::plugin::*;
use rhai::{Array, Blob, Engine, FnPtr, Map, NativeCallContext, Scope, AST, INT};

use crate::navigate::{Navigate, Target, ViewJumpBy};
use crate::prompt;
use crate::terminal;
use crate::tree::NodePath;

struct ScriptFn(FnPtr, usize);
#[derive(Clone, Copy)]
pub struct ScriptFnRef(usize);

pub struct Scripting {
    sourced: Vec<Option<AST>>,
    script_fns: Vec<Option<ScriptFn>>,
    user_script: Option<PathBuf>,
}

impl Scripting {
    pub fn new(user_script: Option<PathBuf>) -> Self {
        Self {
            sourced: Vec::new(),
            script_fns: Vec::new(),
            user_script,
        }
    }
}

#[derive(Clone)]
struct Api {
    nav: Rc<RefCell<Navigate>>,
    current_sourced: usize,
}

impl Api {
    pub fn new(nav: Navigate) -> Self {
        Self {
            nav: Rc::new(RefCell::new(nav)),
            current_sourced: 0, // 0 is user_script or defaults.rhai
        }
    }

    #[inline]
    pub fn m(&mut self) -> RefMut<'_, Navigate> {
        self.nav.borrow_mut()
    }
}

#[derive(Clone)]
struct MakeUncallable;

// pub fn {{{

pub fn make_engine() -> Engine {
    let mut engine = Engine::new();
    engine.register_global_module(exported_module!(api).into());
    // TODO
    //engine.on_print();
    //engine.on_debug();
    engine
}

pub fn main_loop(mut nav: Navigate) -> Result<(), String> {
    let user_script = nav.scripting.user_script.take();

    let engine = make_engine();
    // TODO: export `defaults_keys` and maybe even `default_init`
    //       so it's accessible from custom user scripts
    let mut scope = Scope::new();

    let ast = if let Some(file) = user_script {
        engine
            .compile_file(file)
            .expect("somethin about user script not valid")
    } else {
        engine.compile(include_str!("../defaults.rhai")).unwrap()
    };

    engine
        .eval_ast_with_scope::<()>(scope.push("api", Api::new(nav)), &ast)
        .expect("somethin about user script not valid runtime");
    scope
        .get_value_mut::<Api>("api")
        .unwrap()
        .nav
        .borrow_mut()
        .scripting
        .sourced
        .push(Some(ast));

    terminal::cursor_off();
    terminal::mouse_on();
    terminal::altscreen_on();

    let exit = match engine
        .eval_with_scope::<String>(
            scope.push("uncallable_token", MakeUncallable),
            "loop { break api._tick(uncallable_token) ?? continue }",
        )
        .unwrap()
    {
        it if it.is_empty() => Ok(()),
        text => Err(text),
    };

    terminal::cursor_on();
    terminal::mouse_off();
    terminal::altscreen_off();

    exit
}

// }}}

// priv helpers {{{

type ApiResult<T> = Result<T, Box<EvalAltResult>>;

#[inline]
fn host_path(path: &Array) -> Result<Vec<usize>, &'static str> {
    path.iter()
        .map(|d| d.as_int().map(|k| k as usize))
        .collect()
}

#[inline]
fn script_path(path: &[usize]) -> Array {
    path.iter().map(|k| (*k as INT).into()).collect()
}

#[inline]
fn with_opt_unit<T: Clone + 'static>(f: impl FnOnce() -> Option<T>) -> Dynamic {
    f().map(Dynamic::from).unwrap_or_default()
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

// }}}

#[export_module]
mod api {
    pub fn _tick(cc: NativeCallContext, api: &mut Api, _: MakeUncallable) -> Dynamic {
        let mut nav = api.m();

        let buf = nav.to_string();
        eprint!("{buf}");

        nav.message
            .iter_mut()
            .map(|s| {
                if let Some(n) = s.find('\n') {
                    s.truncate(n);
                }
            })
            .count();

        let Some(action) = nav.input.tick() else {
            return Dynamic::UNIT;
        };
        let (fn_ptr, ast_ref, ast) = nav.scripting.script_fns[action.0]
            .take()
            .and_then(|ScriptFn(fn_ptr, ast_ref)| {
                nav.scripting.sourced[ast_ref]
                    .take()
                    .map(|ast| (fn_ptr, ast_ref, ast))
            })
            .expect("gone fishing (tick likely reached from user script)");

        drop(nav);

        _ = fn_ptr
            .call::<Dynamic>(cc.engine(), &ast, (api.clone(),))
            .expect("TODO");

        let mut nav = api.m();
        nav.scripting.sourced[ast_ref] = Some(ast);
        nav.scripting.script_fns[action.0] = Some(ScriptFn(fn_ptr, ast_ref));

        with_opt_unit(|| nav.exit.take())
    }

    // source/eval {{{

    #[rhai_fn(return_raw)]
    pub fn source_text(cc: NativeCallContext, api: &mut Api, text: &str) -> ApiResult<Dynamic> {
        let p_current_sourced = api.current_sourced;
        api.current_sourced = api.nav.borrow().scripting.sourced.len();

        let ast = cc.engine().compile(text).unwrap();
        let mut scope = Scope::new();
        let r = cc
            .engine()
            .eval_ast_with_scope(scope.push("api", api.clone()), &ast)?;

        api.current_sourced = p_current_sourced;
        api.m().scripting.sourced.push(Some(ast));

        Ok(r)
    }

    #[rhai_fn(return_raw)]
    pub fn source(cc: NativeCallContext, api: &mut Api, file: &str) -> ApiResult<Dynamic> {
        let p_current_sourced = api.current_sourced;
        api.current_sourced = api.nav.borrow().scripting.sourced.len();

        let ast = cc.engine().compile_file(file.into()).unwrap();
        let mut scope = Scope::new();
        let r = cc
            .engine()
            .eval_ast_with_scope(scope.push("api", api.clone()), &ast)?;

        api.current_sourced = p_current_sourced;
        api.m().scripting.sourced.push(Some(ast));

        Ok(r)
    }

    // }}}

    // misc. {{{

    pub fn help(cc: NativeCallContext, fname: &str) -> Array {
        cc.engine().collect_fn_metadata(
            Some(&cc),
            |info| {
                let matches = match fname.as_bytes() {
                    [.., b'*'] => info.metadata.name.starts_with(&fname[..fname.len() - 1]),
                    [b'*', ..] => info.metadata.name.ends_with(&fname[1..]),
                    _ => info.metadata.name == fname,
                };
                if matches {
                    let mut r = info
                        .metadata
                        .gen_signature(|s| cc.engine().map_type_name(s).into());
                    r.push('\n');
                    if !matches!(fname.as_bytes(), [.., b'*'] | [b'*', ..]) {
                        for line in &info.metadata.comments {
                            r.push_str(&line.replace("///", "   "));
                        }
                        r.push('\n');
                    }
                    Some(r.into())
                } else {
                    None
                }
            },
            true,
        )
    }

    pub fn message(api: &mut Api, w: Dynamic) {
        api.m().message = Some(w.to_string().replace("\n", "\r\n"));
    }

    #[rhai_fn(pure)]
    pub fn provider_name(api: &mut Api) -> String {
        api.nav.borrow().provider_name.clone()
    }

    #[rhai_fn(pure)] // not pure but pure enough
    pub fn suspend(_api: &mut Api) {
        #[cfg(not(windows))]
        {
            terminal::cursor_on();
            terminal::mouse_off();
            terminal::altscreen_off();

            let mut nav = _api.m();
            nav.term.take().map(|t| t.restore());
            unsafe { libc::raise(libc::SIGTSTP) };
            nav.term = terminal::raw_with_panic_hook().ok();

            terminal::cursor_off();
            terminal::mouse_on();
            terminal::altscreen_on();
        }
    }

    pub fn quit(api: &mut Api) {
        quit_text(api, "")
    }
    #[rhai_fn(name = "quit", name = "cquit")]
    pub fn quit_code(api: &mut Api, code: INT) {
        quit_text(api, &code.to_string());
    }
    #[rhai_fn(name = "quit", name = "cquit")]
    pub fn quit_text(api: &mut Api, text: &str) {
        let mut nav = api.m();
        nav.exit = Some(text.into());
        nav.term.take().map(|t| t.restore());
    }

    #[rhai_fn(pure)]
    pub fn mouse_event_pos(api: &mut Api) -> Map {
        let info = api.nav.borrow().input.get_pending_mouse_info();
        let mut r = Map::new();
        r.insert("row".into(), (info.row as INT).into());
        r.insert("col".into(), (info.col as INT).into());
        r
    }

    // }}}

    // mapping {{{

    pub fn map(api: &mut Api, seq: &str, cb: FnPtr) {
        let script_fn = ScriptFn(cb, api.current_sourced);
        let mut nav = api.m();

        let fnref = nav.scripting.script_fns.len();
        nav.scripting.script_fns.push(Some(script_fn));

        nav.input.add_mapping(
            terminal::keytrans(seq).expect("need valid seq something blbl"),
            ScriptFnRef(fnref),
        );
    }

    #[rhai_fn(name = "map")]
    pub fn map_multiple(api: &mut Api, map: Map) {
        let current_sourced = api.current_sourced;
        let mut nav = api.m();

        for (seq, cb) in map {
            let Some(cb) = cb.try_cast() else { continue };
            let script_fn = ScriptFn(cb, current_sourced);

            let fnref = nav.scripting.script_fns.len();
            nav.scripting.script_fns.push(Some(script_fn));

            nav.input.add_mapping(
                terminal::keytrans(&seq).expect("need valid seq something blbl"),
                ScriptFnRef(fnref),
            );
        }
    }

    #[rhai_fn(global)]
    pub fn keytrans(text: &str) -> Dynamic {
        with_opt_unit(|| terminal::keytrans(text))
    }

    #[rhai_fn(global)]
    pub fn keyseqstr(seq: Blob) -> String {
        terminal::keyseqstr(&seq)
    }

    // }}}

    // values (options and registers) {{{

    #[rhai_fn(pure, index_get, name = "value")]
    pub fn get_value(api: &mut Api, name: &str) -> Dynamic {
        if b'&' == name.as_bytes()[0] {
            api.nav.borrow().options.get(&name[1..])
        } else {
            api.nav
                .borrow()
                .registers
                .get(name)
                .and_then(|v| v.last().cloned())
                .map(Dynamic::from)
                .unwrap_or_default()
        }
    }

    #[rhai_fn(pure, name = "value")]
    pub fn get_value_hist(api: &mut Api, name: &str, history: &str) -> Dynamic {
        if "hist" != history {
            return get_value(api, name);
        }
        if b'&' == name.as_bytes()[0] {
            let r = api.nav.borrow().options.get(&name[1..]);
            if !r.is_unit() {
                vec![r].into()
            } else {
                r
            }
        } else {
            api.nav
                .borrow()
                .registers
                .get(name)
                .cloned()
                .map(Dynamic::from)
                .unwrap_or_default()
        }
    }

    #[rhai_fn(return_raw, index_set)]
    pub fn set_value(api: &mut Api, name: &str, value: Dynamic) -> ApiResult<()> {
        if b'&' == name.as_bytes()[0] {
            api.m().options.set(&name[1..], value)?;
        } else {
            api.m()
                .registers
                .entry(name.into())
                .or_default()
                .push(value.into_string()?);
        }
        Ok(())
    }

    // }}}

    // user textual input {{{

    pub fn prompt(cc: NativeCallContext, api: &mut Api, ps: &str, completion: FnPtr) -> Dynamic {
        let mut nav = api.m();
        let history = nav.registers.entry(ps.into()).or_default();

        terminal::cursor_on();
        terminal::mouse_off();
        let res = prompt::prompt(
            ps,
            io::stdin().bytes().map_while(Result::ok),
            io::stderr(),
            history.clone(),
            |line, point| {
                completion
                    .call_within_context(&cc, (line.to_string(), point))
                    .unwrap_or_default()
            },
        );
        terminal::cursor_off();
        terminal::mouse_on();

        let Some(r) = res else { return Dynamic::UNIT };
        history.push(r.clone());
        r.into()
    }

    #[rhai_fn(global)]
    pub fn prompt_split(line: &str, point: INT) -> Map {
        let (args, in_arg) = prompt::split(line, point as usize);
        let mut r = Map::new();
        r.insert("args".into(), args.into());
        r.insert("in_arg".into(), (in_arg as INT).into());
        r
    }
    #[rhai_fn(global, name = "prompt_split")]
    pub fn prompt_split_vec(line: &str) -> Vec<String> {
        prompt::split(line, 0).0
    }

    // }}}

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
            let nav = api.nav.borrow();

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
