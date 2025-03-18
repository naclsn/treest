mod navigate;
mod options;
mod prompt;
mod providers;
mod terminal;
mod tree;
mod macros;

fn main() -> anyhow::Result<()> {
    Ok(options::Options::parse_env()?.instanciate()?.main_loop()?)
}
