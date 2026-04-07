#![deny(dead_code, unused_variables, unreachable_code, unused_imports)]

use anyhow::bail;
use clap::Parser;
pub(crate) mod utils;
#[cfg(feature = "repl")]
use utils::repl::Repl;
use utils::{
    args::{Args, Commands},
    client::{CallResult, InteractiveCommandClient},
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
            json,
        } => {
            let result = InteractiveCommandClient::call(object, function, args, info);
            match result {
                Ok(CallResult::Value(val)) => {
                    if json {
                        println!("{}", serde_json::to_string(&val)?);
                    } else {
                        println!("{val:#}");
                    }
                }
                Ok(CallResult::Text(text)) => {
                    if json {
                        println!("{}", serde_json::to_string(&text)?);
                    } else {
                        println!("{text}");
                    }
                }
                Err(err) => bail!("{err}"),
            };
            Ok(())
        }
        #[cfg(feature = "repl")]
        Commands::Repl => {
            let mut repl = Repl::new();
            repl.run()
        }
    }
}
