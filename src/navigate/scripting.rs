use std::cell::{RefCell, RefMut};
use std::io::{self, Read};
use std::path::PathBuf;
use std::rc::Rc;

use rhai::plugin::*;
use rhai::{Engine, FnPtr, Map, NativeCallContext, Scope, AST, INT};

use crate::navigate::{Navigate, Target, ViewJumpBy};
use crate::prompt;
use crate::terminal;

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
            current_sourced: 0, // 0 is user_script or default.rhai
        }
    }

    #[inline]
    pub fn m(&mut self) -> RefMut<'_, Navigate> {
        self.nav.borrow_mut()
    }
}

#[derive(Clone)]
struct MakeUncallable;

pub fn main_loop(mut nav: Navigate) {
    let user_script = nav.scripting.user_script.take();

    let mut engine = Engine::new();
    engine.register_global_module(exported_module!(api).into());

    let ast = if let Some(file) = user_script {
        engine
            .compile_file(file)
            .expect("somethin about user script not valid")
    } else {
        engine.compile(include_str!("default.rhai")).unwrap()
    };

    let mut scope = Scope::new();
    engine
        .eval_ast_with_scope::<()>(scope.push("api", Api::new(nav)), &ast)
        .unwrap();
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
                api.hi("fell out");
            "#,
        )
        .unwrap();
}

#[export_module]
mod api {
    pub fn hi(api: &mut Api, w: Dynamic) {
        api.m().message = Some(format!("hellloo {w:#?}").replace("\n", "\r\n"));
        //panic!("hellloo {w}");
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
                .unwrap();

            let mut nav = api.m();
            nav.scripting.sourced[ast_ref] = Some(ast);
            nav.scripting.script_fns[action.0] = Some(ScriptFn(fn_ptr, ast_ref));
        }
    }

    pub fn source_text(cc: NativeCallContext, api: &mut Api, text: &str) -> Dynamic {
        let p_current_sourced = api.current_sourced;
        api.current_sourced = api.nav.borrow().scripting.sourced.len();

        let ast = cc.engine().compile(text).unwrap();
        let mut scope = Scope::new();
        let r = cc
            .engine()
            .eval_ast_with_scope(scope.push("api", api.clone()), &ast)
            .unwrap();

        api.current_sourced = p_current_sourced;
        api.m().scripting.sourced.push(Some(ast));

        r
    }

    pub fn source(cc: NativeCallContext, api: &mut Api, file: &str) -> Dynamic {
        let p_current_sourced = api.current_sourced;
        api.current_sourced = api.nav.borrow().scripting.sourced.len();

        let ast = cc.engine().compile_file(file.into()).unwrap();
        let mut scope = Scope::new();
        let r = cc
            .engine()
            .eval_ast_with_scope(scope.push("api", api.clone()), &ast)
            .unwrap();

        api.current_sourced = p_current_sourced;
        api.m().scripting.sourced.push(Some(ast));

        r
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

    #[rhai_fn(global)]
    pub fn keytrans(text: &str) -> Dynamic {
        terminal::keytrans(text)
            .map(Dynamic::from)
            .unwrap_or(Dynamic::UNIT)
    }

    #[rhai_fn(global)]
    pub fn keyseqstr(seq: Vec<u8>) -> String {
        terminal::keyseqstr(&seq)
    }

    #[rhai_fn(pure, index_get)]
    pub fn get_option(api: &mut Api, name: &str) -> Dynamic {
        api.m().options.get(name)
    }

    #[rhai_fn(return_raw, index_set)]
    pub fn set_option(api: &mut Api, name: &str, value: Dynamic) -> Result<(), Box<EvalAltResult>> {
        Ok(api.m().options.set(name, value)?)
    }

    #[rhai_fn(pure)]
    pub fn mouse_event_pos(api: &mut Api) -> Map {
        let info = api.nav.borrow().input.get_pending_mouse_info();
        let mut r = Map::new();
        r.insert("row".into(), Dynamic::from(info.row as INT));
        r.insert("col".into(), Dynamic::from(info.col as INT));
        r
    }

    pub fn prompt(cc: NativeCallContext, api: &mut Api, ps: &str, completion: FnPtr) -> Dynamic {
        let mut nav = api.m();
        let history = nav.prompt_history.entry(ps.into()).or_default();

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
        r.insert("in_arg".into(), Dynamic::from(in_arg as INT));
        r
    }
    #[rhai_fn(global, name = "prompt_split")]
    pub fn prompt_split_vec(line: &str) -> Vec<String> {
        prompt::split(line, 0).0
    }

    pub fn fold(api: &mut Api, is: bool, path: Vec<INT>) {
        api.m().set_folded(
            Target::Path(&path.iter().map(|k| *k as usize).collect::<Vec<_>>()),
            is,
        );
    }
    #[rhai_fn(name = "fold")]
    pub fn fold_cursor(api: &mut Api, is: bool) {
        api.m().set_folded(Target::Cursor, is);
    }
    pub fn folded(api: &mut Api, path: Vec<INT>) -> bool {
        api.m().get_folded(Target::Path(
            &path.iter().map(|k| *k as usize).collect::<Vec<_>>(),
        ))
    }
    #[rhai_fn(name = "folded")]
    pub fn folded_cursor(api: &mut Api) -> bool {
        api.m().get_folded(Target::Cursor)
    }

    pub fn mark(api: &mut Api, is: bool, path: Vec<INT>) {
        api.m().set_marked(
            Target::Path(&path.iter().map(|k| *k as usize).collect::<Vec<_>>()),
            is,
        );
    }
    #[rhai_fn(name = "mark")]
    pub fn mark_cursor(api: &mut Api, is: bool) {
        api.m().set_marked(Target::Cursor, is);
    }
    pub fn marked(api: &mut Api, path: Vec<INT>) -> bool {
        api.m().get_marked(Target::Path(
            &path.iter().map(|k| *k as usize).collect::<Vec<_>>(),
        ))
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
}
