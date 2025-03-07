use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use rhai::{plugin::*, Engine, FnPtr, NativeCallContext, Scope, AST};

use crate::terminal;

use super::Navigate;

pub struct Scripting {
    //engine: Option<Engine>, // `.take`n during evaluation
    sourced: Vec<Option<AST>>,
    script_fns: Vec<Option<ScriptFn>>,
}

struct ScriptFn(FnPtr, usize);
#[derive(Clone, Copy)]
pub struct ScriptFnRef(usize);

/// 'scope: lives the duration of the call into script-land
#[derive(Clone)]
struct Api {
    //nav: Rc<RefCell<&'scope mut Navigate>>,
    nav: Rc<RefCell<Navigate>>,
    current_sourced: usize,
}

impl Scripting {
    pub fn new() -> Self {
        //let mut engine = Engine::new();
        //engine
        //    .register_type::<Api>()
        //    .register_fn("hi", Api::hi)
        //    .register_fn("register", Api::register);
        Self {
            //engine: Some(engine),
            sourced: Vec::new(),
            script_fns: Vec::new(),
        }
    }
}

/*
#[inline]
fn bidoof<R>(nav: &mut Navigate, dobido: impl FnOnce(&Engine, &mut Scope) -> R) -> R {
    let engine = nav
        .scripting
        .engine
        .take()
        .expect("run_callback reached from within run_callback");

    let mut scope = Scope::new();
    scope.push_constant(
        "api",
        // XXX: casts lifetime away
        //      this is obviously a poor bandage but it'll do for now;
        //      the thing is that `nav` will never be pysically moved
        //      from within script-land
        //      will change that when it blows a hole in my foot
        Api {
            //nav: Rc::new(RefCell::new(unsafe { &mut *(nav as *mut Navigate) })),
            nav: nav as *mut Navigate,
            current_sourced: nav.scripting.sourced.len(),
        },
    );

    let r = dobido(&engine, &mut scope);
    nav.scripting.engine = Some(engine);
    r
}

pub fn source(nav: &mut Navigate, text: impl AsRef<str>) {
    let ast = bidoof(nav, |engine, scope| {
        let ast = engine.compile(text).unwrap();
        engine.eval_ast_with_scope::<()>(scope, &ast).unwrap();
        ast
    });
    nav.scripting.sourced.push(ast);
}

pub fn source_file(nav: &mut Navigate, path: impl AsRef<Path>) {
    let ast = bidoof(nav, |engine, scope| {
        let ast = engine.compile_file(path.as_ref().into()).unwrap();
        engine.eval_ast_with_scope::<()>(scope, &ast).unwrap();
        ast
    });
    nav.scripting.sourced.push(ast);
}

pub fn run_callback(nav: &mut Navigate, cb: &ScriptFn) {
    cb.0.call::<()>(
        &nav.scripting.engine.as_ref().unwrap(),
        &nav.scripting.sourced[cb.1],
        (),
    )
    .unwrap();
}
*/

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
        .register_fn("register", Api::register)
        .register_fn("_tick", |cc: NativeCallContext, api: &mut Api| api.tick(cc))
        .eval_with_scope::<()>(
            &mut scope,
            r#"
                api.source(`
                    api.register("hi", |api| api.hi("heeeeeeeere"));
                `);
                loop {
                    api._tick();
                }
                api.hi("fell out");
            "#,
        )
        .unwrap();
}

impl Api {
    fn hi(&mut self, w: &str) {
        self.nav.borrow_mut().message = Some(format!("hellloo {w}"));
        //panic!("hellloo {w}");
    }

    fn tick(&mut self, cc: NativeCallContext) {
        eprintln!("LSKDJFKLSDJF");
        let mut nav = self.nav.borrow_mut();

        let Some(action) = nav.tick() else { return };

        let ScriptFn(fn_ptr, ast_ref) = nav.scripting.script_fns[action.0].take().expect("gone fishing (tick reached from user script)");
        let ast = nav.scripting.sourced[ast_ref].take().expect("gone fishing (tick reached from user script)");

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
}
