use serde_json;
use std::env;

mod node;
mod tree;
mod view;

fn main() {
    let mut root = tree::Tree::new(env::current_dir().unwrap()).expect("could not unfold root");

    view::View::new(&mut root)
        .down("src".into())
        .unwrap()
        .down("main.rs".into())
        .unwrap()
        .mark();

    for arg in env::args_os().skip(1) {
        root.unfold_at(arg.into()).expect("could not unfold path");
    }

    print!("{root}");
    print!("{}", serde_json::to_string(&root).unwrap());
}
