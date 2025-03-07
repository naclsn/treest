use std::env;
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

    let nav = match providers::select(&arg, args.next().as_deref()) {
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
    rst_term();
}
