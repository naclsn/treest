use std::env;
use std::fs::File;
use std::io::{self, Read};
use std::panic;
use std::process;

mod navigate;
mod prompt;
mod providers;
mod stabvec;
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

fn main() {
    let mut input = match File::open("/dev/tty") {
        Ok(f) => Box::new(f) as Box<dyn Read>,
        Err(_) => Box::new(io::stdin()),
    }
    .bytes()
    .map_while(Result::ok);

    let mut nav = match providers::fs::Fs::new(".") {
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

    eprint!("{nav}");
    loop {
        let Some(byte) = input.next() else { break };
        nav.feed(byte);

        let buf = nav.to_string();
        eprint!("{buf}");
    }

    rst_term();
}

/*
#[derive(Clone)]
struct Api();

impl Api {
    fn new() -> Self {
        Self()
    }
    fn prompt(&mut self, ps: &str) -> Option<Vec<String>> {
        prompt::prompt(
            ps,
            io::stdin().bytes().map_while(Result::ok),
            io::stderr(),
            |_, _| vec![],
        )
    }
}

fn main() {
    use tree::Provider;
    struct Bidoof(Box<dyn Provider>);
}

fn main4() -> Result<(), Box<dyn std::error::Error>> {
    use rhai::{Engine, Scope};

    let mut engine = Engine::new();
    engine
        .register_type::<Api>()
        //.register_fn("__new_api", Api::new)
        .register_fn("prompt", Api::prompt);
    let mut scope = Scope::new();
    let mut api = Api::new();
    scope.push_constant("api", &api);

    set_term();
    engine.eval_with_scope(
        &mut scope,
        r#"
        print("hi");
        //let api = __new_api();
        print(api.prompt("butts "));
        api = "crap";
        ()
    "#,
    )?;

    println!("api now: {:?}", scope.get("api"));

    //for key in io::stdin().bytes().map_while(Result::ok) {
    //    match key {
    //        3 | b'q' => break,
    //        b':' => (),
    //        _ => (),
    //    }
    //}
    rst_term();

    Ok(())
}

fn main3() {
    set_term();
    while let Some(parts) = prompt::prompt(
        "hello ",
        io::stdin().bytes().map_while(Result::ok),
        io::stderr(),
        |_, _| vec![],
    ) {
        println!("{parts:?}");
        if parts[0].is_empty() {
            break;
        }
    }
    rst_term();
}

fn main2() -> Result<(), Box<dyn std::error::Error>> {
    use rhai::Engine;

    // Define external function
    fn compute_something(x: i64) -> bool {
        (x % 40) == 0
    }

    // Create scripting engine
    let mut engine = Engine::new();

    engine.register_fn("compute", compute_something);

    // Evaluate the script, expecting a 'bool' result
    let result: bool = engine.eval_file("my_script.rhai".into())?;

    assert_eq!(result, true);

    Ok(())
}

fn main1() {
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

    let mut nav = Navigate::new(match providers::select(&arg, args.next().as_deref()) {
        Ok(prov) => prov,
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
    });

    let phook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        rst_term();
        phook(info)
    }));

    set_term();

    eprint!("{nav}");
    loop {
        let Some(byte) = input.next() else { break };
        nav.feed(byte);

        let buf = nav.to_string();
        eprint!("{buf}");
    }

    rst_term();
}
*/
