use std::cell::RefCell;
use std::fs::File;
use std::io::{self, Read};
use std::rc::Rc;

use rhai::{plugin::*, Engine, FnPtr, NativeCallContext, Scope, AST};

use crate::navigate::Navigate;
use crate::prompt;
use crate::terminal;

pub struct Scripting {
    sourced: Vec<Option<AST>>,
    script_fns: Vec<Option<ScriptFn>>,
}

struct ScriptFn(FnPtr, usize);
#[derive(Clone, Copy)]
pub struct ScriptFnRef(usize);

/// 'scope: lives the duration of the call into script-land
#[derive(Clone)]
struct Api {
    nav: Rc<RefCell<Navigate>>,
    current_sourced: usize,
}

impl Scripting {
    pub fn new() -> Self {
        Self {
            sourced: Vec::new(),
            script_fns: Vec::new(),
        }
    }
}

pub fn main_loop(nav: Navigate) {
    let mut scope = Scope::new();
    scope.push(
        "api",
        Api {
            nav: Rc::new(RefCell::new(nav)),
            current_sourced: usize::MAX,
        },
    );

    let mut engine = Engine::new();
    engine
        .register_type::<Api>()
        .register_fn("hi", Api::hi)
        .register_fn(
            "source",
            |cc: NativeCallContext, api: &mut Api, text: &str| api.source(cc, text),
        )
        .register_fn(
            "source_file",
            |cc: NativeCallContext, api: &mut Api, file: &str| api.source_file(cc, file),
        )
        .register_fn("register", Api::register)
        .register_fn("prompt", Api::prompt)
        .register_fn("_tick", |cc: NativeCallContext, api: &mut Api| api.tick(cc))
        .eval_with_scope::<()>(
            &mut scope,
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
                    api._tick();
                }
                api.hi("fell out");
            "##,
        )
        .unwrap();
}

impl Api {
    fn hi(&mut self, w: &str) {
        self.nav.borrow_mut().message = Some(format!("hellloo {w}"));
        //panic!("hellloo {w}");
    }

    /* exported but shoudnt be called */
    fn tick(&mut self, cc: NativeCallContext) {
        let mut nav = self.nav.borrow_mut();
        eprint!("{}", nav.to_string());

        if let Some(action) = nav.input.tick() {
            drop(nav);
            self.run_callback(cc, action);
        }
    }

    /* not exported */
    fn run_callback(&mut self, cc: NativeCallContext, action: ScriptFnRef) {
        let mut nav = self.nav.borrow_mut();

        let (fn_ptr, ast_ref, ast) = nav.scripting.script_fns[action.0]
            .take()
            .and_then(|ScriptFn(fn_ptr, ast_ref)| {
                nav.scripting.sourced[ast_ref]
                    .take()
                    .map(|ast| (fn_ptr, ast_ref, ast))
            })
            .expect("gone fishing (likely run_callback reached from user script)");

        // this is needed so nested calls to api functions can work
        // the explicit drop is kept for explicitness/doc
        drop(nav);

        fn_ptr
            .call::<()>(cc.engine(), &ast, (self.clone(),))
            .unwrap();

        let mut nav = self.nav.borrow_mut();
        nav.scripting.sourced[ast_ref] = Some(ast);
        nav.scripting.script_fns[action.0] = Some(ScriptFn(fn_ptr, ast_ref));
    }

    fn source(&mut self, cc: NativeCallContext, text: &str) {
        let p_current_sourced = self.current_sourced;
        self.current_sourced = self.nav.borrow().scripting.sourced.len();

        let ast = cc.engine().compile(text).unwrap();
        let mut scope = Scope::new();
        cc.engine()
            .eval_ast_with_scope::<()>(scope.push("api", self.clone()), &ast)
            .unwrap();

        self.current_sourced = p_current_sourced;
        self.nav.borrow_mut().scripting.sourced.push(Some(ast));
    }

    fn source_file(&mut self, cc: NativeCallContext, file: &str) {
        let p_current_sourced = self.current_sourced;
        self.current_sourced = self.nav.borrow().scripting.sourced.len();

        let ast = cc.engine().compile_file(file.into()).unwrap();
        let mut scope = Scope::new();
        cc.engine()
            .eval_ast_with_scope::<()>(scope.push("api", self.clone()), &ast)
            .unwrap();

        self.current_sourced = p_current_sourced;
        self.nav.borrow_mut().scripting.sourced.push(Some(ast));
    }

    fn register(&mut self, seq: &str, cb: FnPtr) {
        let script_fn = ScriptFn(cb, self.current_sourced);
        let mut nav = self.nav.borrow_mut();

        let fnref = nav.scripting.script_fns.len();
        nav.scripting.script_fns.push(Some(script_fn));

        nav.input.add_mapping(
            terminal::keytrans(seq).expect("need valid seq something blbl"),
            ScriptFnRef(fnref),
        );
    }

    fn prompt(&mut self, ps: &str) -> String {
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
