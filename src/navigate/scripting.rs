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

pub fn make_engine() -> Engine {
    let mut engine = Engine::new();
    engine.register_global_module(exported_module!(api).into());
    engine
}

pub fn main_loop(mut nav: Navigate) {
    let user_script = nav.scripting.user_script.take();

    let engine = make_engine();

    let ast = if let Some(file) = user_script {
        engine
            .compile_file(file)
            .expect("somethin about user script not valid")
    } else {
        engine.compile(include_str!("../defaults.rhai")).unwrap()
    };

    let mut scope = Scope::new();
    engine
        .eval_ast_with_scope::<()>(scope.push("api", Api::new(nav)), &ast)
        .expect(
            "somethin about user script not valid runtime, 'cause defaults.rhai should be valid",
        );
    scope
        .get_value_mut::<Api>("api")
        .unwrap()
        .nav
        .borrow_mut()
        .scripting
        .sourced
        .push(Some(ast));

    engine
        .eval_with_scope::<()>(
            scope.push("uncallable_token", MakeUncallable),
            r#"
                api.fold(false);
                loop {
                    api._tick(uncallable_token);
                }
            "#,
        )
        .unwrap();
}

type ApiResult<T> = Result<T, Box<EvalAltResult>>;

fn host_path(path: &Array) -> Result<Vec<usize>, &'static str> {
    path.iter()
        .map(|d| d.as_int().map(|k| k as usize))
        .collect()
}

fn script_path(path: &[usize]) -> Array {
    path.iter().map(|k| (*k as INT).into()).collect()
}

#[export_module]
mod api {
    pub fn message(api: &mut Api, w: Dynamic) {
        api.m().message = Some(w.to_string().replace("\n", "\r\n"));
    }

    pub fn _tick(cc: NativeCallContext, api: &mut Api, _: MakeUncallable) {
        let mut nav = api.m();

        let buf = nav.to_string();
        eprint!("{buf}");

        if let Some(action) = nav.input.tick() {
            let (fn_ptr, ast_ref, ast) = nav.scripting.script_fns[action.0]
                .take()
                .and_then(|ScriptFn(fn_ptr, ast_ref)| {
                    nav.scripting.sourced[ast_ref]
                        .take()
                        .map(|ast| (fn_ptr, ast_ref, ast))
                })
                .expect("gone fishing (tick likely reached from user script)");

            // api is needed so nested calls to api functions can work
            // the explicit drop is kept for explicitness/doc
            drop(nav);

            fn_ptr
                .call::<()>(cc.engine(), &ast, (api.clone(),))
                .expect("TODO");

            let mut nav = api.m();
            nav.scripting.sourced[ast_ref] = Some(ast);
            nav.scripting.script_fns[action.0] = Some(ScriptFn(fn_ptr, ast_ref));
        }
    }

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

    pub fn register(api: &mut Api, seq: &str, cb: FnPtr) {
        let script_fn = ScriptFn(cb, api.current_sourced);
        let mut nav = api.m();

        let fnref = nav.scripting.script_fns.len();
        nav.scripting.script_fns.push(Some(script_fn));

        nav.input.add_mapping(
            terminal::keytrans(seq).expect("need valid seq something blbl"),
            ScriptFnRef(fnref),
        );
    }

    #[rhai_fn(name = "register")]
    pub fn register_multiple(api: &mut Api, map: Map) {
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
        terminal::keytrans(text)
            .map(Dynamic::from_blob)
            .unwrap_or_default()
    }

    #[rhai_fn(global)]
    pub fn keyseqstr(seq: Blob) -> String {
        terminal::keyseqstr(&seq)
    }

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

    #[rhai_fn(pure)]
    pub fn provider_name(api: &mut Api) -> String {
        api.nav.borrow().provider_name.clone()
    }

    #[rhai_fn(pure)]
    pub fn mouse_event_pos(api: &mut Api) -> Map {
        let info = api.nav.borrow().input.get_pending_mouse_info();
        let mut r = Map::new();
        r.insert("row".into(), (info.row as INT).into());
        r.insert("col".into(), (info.col as INT).into());
        r
    }

    pub fn prompt(cc: NativeCallContext, api: &mut Api, ps: &str, completion: FnPtr) -> Dynamic {
        let mut nav = api.m();
        let history = nav.registers.entry(ps.into()).or_default();

        terminal::mouse_off();
        terminal::cursor_on();
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
        terminal::mouse_on();
        terminal::cursor_off();

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

    pub fn quit(_: &mut Api) {
        // TODO: quit
        panic!("haha");
    }

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

    // searches {{{
    // xxx: searches can only search from cursor...
    // xxx: code dupe / unreadable control flow
    // TODO/FIXME: bunch of int underflow / bound not checked

    pub fn search_next(api: &mut Api, q: &str, wrapping: &str) -> Dynamic {
        api.m()
            .registers
            .entry("/".into())
            .or_default()
            .push(q.into());
        search_next_already(api, wrapping)
    }
    #[rhai_fn(name = "search_next")]
    pub fn search_next_already(api: &mut Api, wrapping: &str) -> Dynamic {
        let nav = api.nav.borrow();

        if nav.is_cursor_root() {
            return Dynamic::UNIT; // reason: not found
        };
        let [parent_path @ .., current] = nav.cursor() else {
            unreachable!();
        };
        let search_from = *current + 1;

        let parent = nav.tree.resolve(parent_path);
        let chs = parent.last().unwrap().children().unwrap();

        let Some(s) = nav.registers.get("/").and_then(|v| v.last()) else {
            return Dynamic::UNIT; // reason: no search to repeat
        };

        match chs[search_from..].iter().position(|node| {
            nav.provider
                .display(&NodePath {
                    head: &parent,
                    tail: node,
                })
                .contains(s)
        }) {
            Some(found) => {
                let mut r = script_path(parent_path);
                r.push(((search_from + found) as INT).into());
                r.into()
            }
            None if "wrap" == wrapping => {
                let search_until = *current - 1;
                let Some(found) = chs[..=search_until].iter().position(|node| {
                    nav.provider
                        .display(&NodePath {
                            head: &parent,
                            tail: node,
                        })
                        .contains(s)
                }) else {
                    return Dynamic::UNIT; // reason: not found
                };
                let mut r = script_path(parent_path);
                r.push((found as INT).into());
                r.into()
            }
            None => Dynamic::UNIT, // reason: not found
        }
    }
    #[rhai_fn(name = "search_next")]
    pub fn search_next_already_sat(api: &mut Api) -> Dynamic {
        search_next_already(api, "sat")
    }

    pub fn search_prev(api: &mut Api, q: &str, wrapping: &str) -> Dynamic {
        api.m()
            .registers
            .entry("/".into())
            .or_default()
            .push(q.into());
        search_prev_already(api, wrapping)
    }
    #[rhai_fn(name = "search_prev")]
    pub fn search_prev_already(api: &mut Api, wrapping: &str) -> Dynamic {
        let nav = api.nav.borrow();

        if nav.is_cursor_root() {
            return Dynamic::UNIT; // reason: not found
        };
        let [parent_path @ .., current] = nav.cursor() else {
            unreachable!();
        };
        let search_until = *current - 1;

        let parent = nav.tree.resolve(parent_path);
        let chs = parent.last().unwrap().children().unwrap();

        let Some(s) = nav.registers.get("/").and_then(|v| v.last()) else {
            return Dynamic::UNIT; // reason: no search to repeat
        };

        match chs[..=search_until].iter().rev().position(|node| {
            nav.provider
                .display(&NodePath {
                    head: &parent,
                    tail: node,
                })
                .contains(s)
        }) {
            Some(found) => {
                let mut r = script_path(parent_path);
                r.push(((search_until - found) as INT).into());
                r.into()
            }
            None if "wrap" == wrapping => {
                let search_from = *current + 1;
                let Some(found) = chs[search_from..].iter().rev().position(|node| {
                    nav.provider
                        .display(&NodePath {
                            head: &parent,
                            tail: node,
                        })
                        .contains(s)
                }) else {
                    return Dynamic::UNIT; // reason: not found
                };
                let mut r = script_path(parent_path);
                r.push(((chs.len() - 1 - found) as INT).into());
                r.into()
            }
            None => Dynamic::UNIT, // reason: not found
        }
    }
    #[rhai_fn(name = "search_prev")]
    pub fn search_prev_already_sat(api: &mut Api) -> Dynamic {
        search_prev_already(api, "sat")
    }

    pub fn search_deep(api: &mut Api, q: &str, wrapping: &str) -> Dynamic {
        api.m()
            .registers
            .entry("/".into())
            .or_default()
            .push(q.into());
        search_deep_already(api, wrapping)
    }
    #[rhai_fn(name = "search_deep")]
    pub fn search_deep_already(api: &mut Api, wrapping: &str) -> Dynamic {
        Dynamic::UNIT
    }
    #[rhai_fn(name = "search_deep")]
    pub fn search_deep_already_sat(api: &mut Api) -> Dynamic {
        search_deep_already(api, "sat")
    }

    // }}}
}
