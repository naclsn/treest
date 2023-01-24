use std::env;

mod tree;

fn main() {
    let mut root = tree::Tree::new(env::current_dir().unwrap()).expect("could not unfold root");

    for arg in env::args_os().skip(1) {
        root.unfold_at(arg.into()).expect("could not unfold path");
    }

    print!("{root}");
}
