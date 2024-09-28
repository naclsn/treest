use std::env;
use std::fs::File;
use std::io::{self, Read};
use std::panic;

mod fisovec;
mod navigate;
mod providers;
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
    eprint!("\x1b[?9h\x1b[?25l\x1b[?1049h");
}

fn rst_term() {
    eprint!("\x1b[?9l\x1b[?25h\x1b[?1049l");
    unsafe {
        if let Some(term) = RESTORE.take() {
            term.restore();
        }
    }
}

fn main() {
    let mut args = env::args().skip(1);
    let name = match args.next() {
        Some(list) if "list" == list => {
            for name in providers::NAMES {
                println!("{name}");
            }
            return;
        }
        Some(name) => name,
        None => "fs".into(),
    };

    let input = match File::open("/dev/tty") {
        Ok(f) => Box::new(f) as Box<dyn Read>,
        Err(_) => Box::new(io::stdin()),
    }
    .bytes()
    .map_while(Result::ok);

    let phook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        rst_term();
        phook(info)
    }));

    let arg = args.next().unwrap_or("".into());
    let mut nav = Navigate::new(providers::select(&name, &arg).unwrap());

    set_term();

    eprint!("{nav}");
    for key in input {
        match nav.feed(key) {
            State::Continue | State::Pending(_) => {
                let buf = nav.to_string();
                eprint!("{buf}");
            }
            State::Quit => break,
        }
    }

    rst_term();
}
