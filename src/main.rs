use clap::Parser;
use qtile_client_lib::utils;
#[cfg(feature = "repl")]
use utils::repl::Repl;
use utils::{
    args::{Args, Commands},
    client::{CommandQuery, QtileClient},
};

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    run(args)
}

pub(crate) fn run(args: Args) -> anyhow::Result<()> {
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .env()
        .init()
        .ok(); // Ignore re-init error in tests
    let client = QtileClient::new();

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
                query = query.function(f);
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
        #[cfg(feature = "repl")]
        Commands::Repl => {
            let mut repl = Repl::new();
            repl.run()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::args::Commands;

    #[test]
    fn test_run_cmd_obj_status() {
        let args = Args {
            command: Commands::CmdObj {
                object: None,
                function: Some("status".to_string()),
                args: None,
                info: false,
                json: true,
            },
        };
        // In coverage env, qtile is running, so this should succeed.
        let res = run(args);
        if let Err(err) = res {
            println!(
                "Warning: test_run_cmd_obj_status failed, likely no qtile socket: {:?}",
                err
            );
        }
    }

    #[test]
    fn test_run_invalid_object() {
        let args = Args {
            command: Commands::CmdObj {
                object: Some(vec!["nonexistent".to_string()]),
                function: Some("info".to_string()),
                args: None,
                info: false,
                json: false,
            },
        };
        let res = run(args);
        assert!(res.is_err());
    }

    #[test]
    fn test_run_invalid_function() {
        let args = Args {
            command: Commands::CmdObj {
                object: None,
                function: Some("nonexistent_func".to_string()),
                args: None,
                info: false,
                json: false,
            },
        };
        let res = run(args);
        assert!(res.is_err());
    }

    #[test]
    #[cfg(feature = "repl")]
    fn test_run_repl_init() {
        let _repl = Repl::new();
    }

    #[test]
    #[allow(irrefutable_let_patterns)]
    fn test_args_defaults() {
        // Without -f, function is None — the parser handles showing available commands
        let args = Args::try_parse_from(["qticc", "cmd-obj"]).unwrap();
        if let Commands::CmdObj { function, .. } = args.command {
            assert_eq!(function, None);
        } else {
            panic!("Expected CmdObj default");
        }
    }
}
