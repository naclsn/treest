mod all_the_stuff;
mod app;
mod args;
mod commands;
mod completions;
mod line;
mod node;
mod textblock;
mod tree;
mod view;

use crate::{all_the_stuff::AllTheStuff, args::Args};
use clap::Parser;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let crap = AllTheStuff::new(Args::parse())?;
    crap.run()?;

    // // TODO: get each views' root paths and save individually there
    // //       so that is plays better with reroot
    // if let Some(parent) = save_at.parent() {
    //     if !parent.exists() {
    //         fs::create_dir_all(parent)?;
    //     }
    // }
    // fs::write(&save_at, serde_json::to_string(&app)?)?;

    Ok(())
}
