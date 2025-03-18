use std::env;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=src/navigate/scripting.rs");

    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("help.rs");
    fs::write(
        &dest_path,
        r####"const HELP: &[(&str, &str)] = &[
    ("help", r###"Get a help text about a subject.
`help('help')` would return this text if it was actually implemented.

@param subj string
@return string?
"###),
];"####,
    ).unwrap();
}
