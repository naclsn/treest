use serde_json;
use std::env;

mod node;
mod tree;

fn main() {
    let mut root = tree::Tree::new(env::current_dir().unwrap()).expect("could not unfold root");

    let some = root.at("src/main.rs".into()).expect("something wrong");
    some.mark(true);

    drop(some);

    for arg in env::args_os().skip(1) {
        root.unfold_at(arg.into()).expect("could not unfold path");
    }

    print!("{root}");
    print!("{}", serde_json::to_string(&root).unwrap());
}
