use std::env;
use std::process;
use std::fs::File;
use std::io::{self, Read};
use std::panic;

mod fisovec;
mod navigate;
mod prompt;
mod providers;
mod reqres;
mod stabvec;
mod terminal;
mod tree;

use crate::navigate::{Navigate, State};
use crate::terminal::Restore;

static mut RESTORE: Option<Restore> = None;

fn set_term() {
    unsafe {
        if RESTORE.is_none() {
            RESTORE = Some(terminal::raw().unwrap());
        }
    }
    eprint!("\x1b[?25l\x1b[?1000h\x1b[?1049h");
}

fn rst_term() {
    eprint!("\x1b[?25h\x1b[?1000l\x1b[?1049l");
    unsafe {
        if let Some(term) = RESTORE.take() {
            term.restore();
        }
    }
}

fn temporary_completion(args: Vec<&str>, in_arg: usize) -> Vec<String> {
    if 0 == in_arg {
        &["set", "quit"][..]
    } else {
        match args[0] {
            "set" => &["mouse", "altscreen", "pretty", "onlychild"][..],
            _ => &[][..],
        }
    }
    .iter()
    .filter_map(|word| word.strip_prefix(args[in_arg]).map(|_| word.to_string()))
    .collect()
}

fn main() {
    let mut args = env::args();
    let prog = args.next().unwrap();
    let arg = match args.next().unwrap_or(".".into()) {
        list if "--list" == list => {
            for name in providers::NAMES {
                println!("{name}");
            }
            return;
        }
        help if "--help" == help => {
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
    while (match &mut nav.state {
        State::Continue(r) => input.next().map(|key| r.process(|()| key)).is_some(),
        State::Prompt(r) => {
            eprint!("\x1b[?25h\x1b[?1000l");
            r.process(|r| {
                let l = prompt::prompt(&r, input.by_ref(), io::stderr(), temporary_completion);
                (r, l)
            });
            eprint!("\x1b[?25l\x1b[?1000h");
            true
        }
        State::ExecStatus(r) => {
            r.process(|(restore, mut r)| {
                if restore {
                    rst_term();
                }
                let r = r.status();
                if restore {
                    set_term();
                }
                r
            });
            true
        }
        State::ExecOutput(r) => {
            r.process(|(restore, mut r)| {
                if restore {
                    rst_term();
                }
                let r = r.output();
                if restore {
                    set_term();
                }
                r
            });
            true
        }
    }) && nav.is_continue()
    {
        let buf = nav.to_string();
        eprint!("{buf}");
    }

    rst_term();
}
