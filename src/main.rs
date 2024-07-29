#![deny(dead_code, unused_variables, unreachable_code, unused_imports)]

use clap::Parser;
pub(crate) mod utils;
use utils::{args::Args, client::ShellClient};

fn main() -> anyhow::Result<()> {
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .env()
        .init()?;
    let args = Args::parse();
    // println!("{:#?}", args);
    ShellClient::call(args)
}
