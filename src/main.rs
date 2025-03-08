use std::env;
use std::panic;
use std::process;

mod navigate;
mod options;
mod prompt;
mod providers;
mod terminal;
mod tree;

use crate::options::Options;
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
    let nav = Options::parse(env::args())
        .unwrap_or_else(|err| {
            eprintln!("{err}");
            process::exit(1);
        })
        .instanciate()
        .unwrap_or_else(|err| {
            eprintln!("Error: {err}.");
            if let Some(err) = err.source() {
                eprintln!("Because {err}.");
                if err.source().is_some() {
                    eprintln!("Because ...");
                }
            }
            process::exit(1);
        });

    let phook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        rst_term();
        phook(info)
    }));

    set_term();
    nav.main_loop();
    rst_term();
}
