use std::io::{self, Read};

mod fisovec;
mod navigate;
mod providers;
mod stabvec;
mod terminal;
mod tree;

use crate::navigate::{Navigate, State};
use crate::providers::fs::Fs;

fn main() {
    let mut nav = Navigate::new(Fs::new(".".into()));

    let term = terminal::raw().unwrap();
    print!("\x1b[?25l\x1b[?1049h{nav}\r\n");

    for key in io::stdin().bytes().map_while(Result::ok) {
        match nav.feed(key) {
            State::Continue => print!("{nav}\r\n"),
            State::Quit => break,
        }
    }

    print!("\x1b[?25h\x1b[?1049l");
    term.restore();
}
