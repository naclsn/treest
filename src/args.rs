use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// path to open at, defaults to current directory
    pub path: Option<PathBuf>,

    /// do not load any existing state for this path
    #[arg(short = 'x', long, default_value_t = false)]
    pub clearstate: bool,

    /// use specified config instead of any existing default ($HOME/.config/treest)
    #[arg(short, long)]
    pub userconf: Option<PathBuf>,

    /// do not use any config
    #[arg(long, default_value_t = false)]
    pub clean: bool,
}
