mod providers;
mod tree;

use crate::providers::fs::Fs;
use crate::tree::Tree;

fn main() {
    let mut app = Tree::new(Fs::new(".".into()));
    println!("{app}");

    app.unfold_at(app.root());
    println!("{app}");

    app.unfold_at(app.at(app.root()).first_child().unwrap());
    app.unfold_at(app.at(app.root()).last_child().unwrap());
    println!("{app}");

    app.fold_at(app.at(app.root()).first_child().unwrap());
    println!("{app}");
}
