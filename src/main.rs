#![allow(dead_code, unused_variables, unreachable_code, unused_imports)]

use clap::Parser;
mod utils;
use utils::{args::Args, client::InteractiveCommandClient};
mod update_qtile;
// use update_qtile::UpdateQtile;

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    // println!("{:#?}", args);
    InteractiveCommandClient::call(args)
    // let _up = UpdateQtile::new();
}
