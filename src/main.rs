mod lua;
mod navigate;
mod options;
mod prompt;
mod provider;
mod terminal;
mod tree;

#[macro_export]
macro_rules! include_etc {
    ($file:expr $(,)?) => {
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/etc/", $file))
    };
}

fn main() -> anyhow::Result<()> {
    Ok(options::Options::parse_env()?.instanciate()?.main_loop()?)
}
