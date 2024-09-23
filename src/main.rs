#![deny(dead_code, unused_variables, unreachable_code, unused_imports)]

use anyhow::bail;
use clap::Parser;
pub(crate) mod utils;
use utils::{
    args::{Args, Commands},
    client::InteractiveCommandClient,
};

fn main() -> anyhow::Result<()> {
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .env()
        .init()?;
    let args = Args::parse();
    match args.command {
        Commands::CmdObj {
            object,
            function,
            args,
            info,
        } => {
            let result = InteractiveCommandClient::call(object, function, args, info);
            match result {
                Ok(result) => println!("{result:#}"),
                Err(err) => bail!("{err}"),
            };
            Ok(())
        }
    }
}
