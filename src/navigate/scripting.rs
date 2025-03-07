use std::cell::RefCell;
use std::io::{self, Read};
use std::rc::Rc;

use rhai::{plugin::*, Engine, FnPtr, NativeCallContext, Scope, AST};

use crate::navigate::Navigate;
use crate::prompt;
use crate::terminal;

struct ScriptFn(FnPtr, usize);
#[derive(Clone, Copy)]
pub struct ScriptFnRef(usize);

pub struct Scripting {
    sourced: Vec<Option<AST>>,
    script_fns: Vec<Option<ScriptFn>>,
}

impl Scripting {
    pub fn new() -> Self {
        Self {
            sourced: Vec::new(),
            script_fns: Vec::new(),
        }
    }
}

#[derive(Clone)]
struct Api {
    nav: Rc<RefCell<Navigate>>,
    current_sourced: usize,
}

#[derive(Clone)]
struct MakeUncallable;

pub fn main_loop(nav: Navigate) {
    let mut scope = Scope::new();
    let api = Api {
        nav: Rc::new(RefCell::new(nav)),
        current_sourced: usize::MAX,
    };
    let mut engine = Engine::new();
    engine
        .register_global_module(exported_module!(api).into())
        .eval_with_scope::<()>(
            scope
                .push("api", api)
                .push("uncallable_token", MakeUncallable),
            r##"
                api.source(#"
                    api.register("hi", |api| api.hi("heeeeeeeere"));
                    api.register(":", |api| {
                        let ans = api.prompt(":");
                        //api.hi(ans.trim());
                        if ans.starts_with("eval ") {
                            api.source(ans[5..]);
                        }
                    });
                "#);
                loop {
                    api._tick(uncallable_token);
                }
                api.hi("fell out");
            "##,
        )
        .unwrap();
}

#[export_module]
mod api {
    pub fn hi(api: &mut Api, w: &str) {
        api.nav.borrow_mut().message = Some(format!("hellloo {w}"));
        //panic!("hellloo {w}");
    }

    pub fn _tick(cc: NativeCallContext, api: &mut Api, _: MakeUncallable) {
        let mut nav = api.nav.borrow_mut();

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

            let mut nav = api.nav.borrow_mut();
            nav.scripting.sourced[ast_ref] = Some(ast);
            nav.scripting.script_fns[action.0] = Some(ScriptFn(fn_ptr, ast_ref));
        }
    }

    pub fn source(cc: NativeCallContext, api: &mut Api, text: &str) {
        let p_current_sourced = api.current_sourced;
        api.current_sourced = api.nav.borrow().scripting.sourced.len();

        let ast = cc.engine().compile(text).unwrap();
        let mut scope = Scope::new();
        cc.engine()
            .eval_ast_with_scope::<()>(scope.push("api", api.clone()), &ast)
            .unwrap();

        api.current_sourced = p_current_sourced;
        api.nav.borrow_mut().scripting.sourced.push(Some(ast));
    }

    pub fn source_file(cc: NativeCallContext, api: &mut Api, file: &str) {
        let p_current_sourced = api.current_sourced;
        api.current_sourced = api.nav.borrow().scripting.sourced.len();

        let ast = cc.engine().compile_file(file.into()).unwrap();
        let mut scope = Scope::new();
        cc.engine()
            .eval_ast_with_scope::<()>(scope.push("api", api.clone()), &ast)
            .unwrap();

        api.current_sourced = p_current_sourced;
        api.nav.borrow_mut().scripting.sourced.push(Some(ast));
    }

    pub fn register(api: &mut Api, seq: &str, cb: FnPtr) {
        let script_fn = ScriptFn(cb, api.current_sourced);
        let mut nav = api.nav.borrow_mut();

        let fnref = nav.scripting.script_fns.len();
        nav.scripting.script_fns.push(Some(script_fn));

        nav.input.add_mapping(
            terminal::keytrans(seq).expect("need valid seq something blbl"),
            ScriptFnRef(fnref),
        );
    }

    #[rhai_fn(pure)]
    pub fn mouse_event_pos(api: &mut Api) -> Option<(u8, u8)> {
        api.nav
            .borrow()
            .input
            .get_pending_mouse_info()
            .map(|info| (info.row, info.col))
    }

    #[rhai_fn(pure)]
    pub fn prompt(_: &mut Api, ps: &str) -> String {
        terminal::mouse_off();
        terminal::cursor_on();
        let res = prompt::prompt(
            ps,
            io::stdin().bytes().map_while(Result::ok),
            io::stderr(),
            |_, _| vec![],
        );
        terminal::mouse_on();
        terminal::cursor_off();
        res.unwrap()
    }
}
