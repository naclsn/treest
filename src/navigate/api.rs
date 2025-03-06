use std::cell::RefCell;
use std::rc::Rc;

use rhai::{plugin::*, Engine, Scope};

use super::Navigate;

use crate::providers::Provider;

/// 'scope: lives the duration of the call into script-land
#[derive(Clone)]
struct Api {
    nav: *mut Navigate,
}

impl Api {
    fn hi(&mut self) {
        panic!("hellloo");
    }
}

pub(super) fn init_engine() -> Engine {
    let mut engine = Engine::new();

    engine
        .register_type::<Api/*<'scope, P>*/>()
        .register_fn("hi", Api::hi);
    engine
}

pub(super) fn run_callback(nav: &mut Navigate, name: &str) {
    let engine = nav
        .engine
        .take()
        .expect("run_callback reached from within run_callback");
    let mut scope = Scope::new();
    engine
        .eval_with_scope::<()>(scope.push_constant("api", Api { nav: nav as *mut Navigate }), "api.hi(); ()")
        .unwrap();
    nav.engine = Some(engine);
}
