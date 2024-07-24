#![allow(dead_code, unused_variables, unreachable_code, unused_imports)]

use clap::Parser;
pub(crate) mod utils;
use utils::{args::Args, client::ShellClient};

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    // println!("{:#?}", args);
    ShellClient::call(args)
}
