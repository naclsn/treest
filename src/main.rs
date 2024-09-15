mod providers;
mod tree;
mod stabvec;
mod navigate;

use crate::providers::fs::Fs;
use crate::navigate::{Navigate, Direction};

fn main() {
    let mut nav = Navigate::new(Fs::new(".".into()));
    println!("{nav}");

    nav.enter();
    println!("{nav}");

    nav.unfold();
    nav.sibling_wrap(Direction::Previous);
    nav.unfold();
    println!("{nav}");

    nav.sibling_wrap(Direction::Next);
    nav.fold();
    println!("{nav}");
}
