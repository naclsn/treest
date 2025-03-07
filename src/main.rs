use std::env;
use std::fs::File;
use std::io::{self, Read};
use std::panic;
use std::process;

mod navigate;
mod prompt;
mod providers;
mod terminal;
mod tree;

use crate::navigate::Navigate;
use crate::terminal::Restore;

static mut RESTORE: Option<Restore> = None;

fn set_term() {
    unsafe {
        if RESTORE.is_none() {
            RESTORE = Some(terminal::raw().unwrap());
        }
    }
    terminal::cursor_off();
    terminal::mouse_on();
    terminal::altscreen_on();
}

fn rst_term() {
    terminal::cursor_on();
    terminal::mouse_off();
    terminal::altscreen_off();
    unsafe {
        if let Some(term) = RESTORE.take() {
            term.restore();
        }
    }
}

fn main0() {
    use rhai::{Engine, Scope, Dynamic, FnPtr};
    #[derive(Clone, Debug)]
    struct Api(Vec<FnPtr>);
    impl Api {
        fn hi(&mut self, www: &str) {
            println!("hi {www}");
        }

        fn register(&mut self, cb: FnPtr) {
            self.0.push(cb.into());
        }
    }

    let mut engine = Engine::new();
    let mut scope = Scope::new();
    scope.push("api", Api(Vec::new()));

    let src = r#"
        api.hi("top");

        fn crap() {
            api.hi("but");
        }

        api.register(|| api.hi("inn"));

        ()
    "#;

    let ast = engine
        .register_type::<Api>()
        .register_fn("hi", Api::hi)
        .register_fn("register", Api::register)
        .compile_with_scope(&mut scope, src)
        //.eval_with_scope::<()>(&mut scope, src)
        .unwrap();
    _ = engine.eval_ast_with_scope::<Dynamic>(&mut scope, &ast).unwrap();

    let api: Api = scope.get("api").unwrap().clone().try_cast_result().unwrap();
    dbg!(&api);

    let f = &api.0[0];
    f.call::<()>(&engine, &ast, ()).unwrap();
    //f.is_anonymous()
    dbg!(&ast);
}

fn main() {
    let mut args = env::args();
    let prog = args.next().unwrap();
    let arg = match args.next().unwrap_or(".".into()) {
        list if "--list" == list || "-l" == list => {
            for name in providers::NAMES {
                println!("{name}");
            }
            return;
        }
        help if "--help" == help || "-h" == help => {
            eprintln!(
                r#"Usage: {prog} [arg [name]]

    Navigate a tree-like space dynamically.

    `arg` is passed to the provider `name`; if `name` is not given
    it's guessed from `arg`. See '--list' for a list of providers.
    Note: if `arg` is not given, it defaults to ".", so "fs" name.
"#
            );
            return;
        }
        dash if "-" == dash => String::new(),
        arg => arg,
    };

    let mut input = match File::open("/dev/tty") {
        Ok(f) => Box::new(f) as Box<dyn Read>,
        Err(_) => Box::new(io::stdin()),
    }
    .bytes()
    .map_while(Result::ok);

    let mut nav = match providers::select(&arg, args.next().as_deref()) {
        Ok(prov) => Navigate::new(prov),
        Err(err) => {
            eprintln!("Error: {err}.");
            if let Some(err) = err.source() {
                eprintln!("Because {err}.");
                if err.source().is_some() {
                    eprintln!("Because ...");
                }
            }
            process::exit(1);
        }
    };

    let phook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        rst_term();
        phook(info)
    }));

    set_term();

    nav.main_loop();
    //eprint!("{nav}");
    //loop {
    //    let Some(byte) = input.next() else { break };
    //    nav.feed(byte);
    //
    //    let buf = nav.to_string();
    //    eprint!("{buf}");
    //}

    rst_term();
}
