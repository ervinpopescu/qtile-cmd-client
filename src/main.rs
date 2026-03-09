#![deny(dead_code, unused_variables, unreachable_code, unused_imports)]

use clap::Parser;
pub(crate) mod utils;
use utils::{
    args::{Args, Commands},
    client::{CommandQuery, QtileClient},
    repl::Repl,
};

fn main() -> anyhow::Result<()> {
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .env()
        .init()?;
    let args = Args::parse();
    let framed = args.framed;
    let client = QtileClient::new(framed);

    match args.command {
        Commands::CmdObj {
            object,
            function,
            args,
            info,
            json,
        } => {
            let mut query = CommandQuery::new().info(info);
            if let Some(o) = object {
                query = query.object(o);
            }
            if let Some(f) = function {
                query = query.function(f.to_string());
            }
            if let Some(a) = args {
                query = query.args(a);
            }

            let result = client.call(query)?;
            if json {
                println!("{}", serde_json::to_string(&result.to_json())?);
            } else {
                println!("{result}");
            }
            Ok(())
        }
        Commands::Repl => {
            let mut repl = Repl::new(framed);
            repl.run()
        }
    }
}
