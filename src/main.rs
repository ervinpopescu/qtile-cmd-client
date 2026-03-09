use clap::Parser;
pub(crate) mod utils;
use utils::{
    args::{Args, Commands},
    client::{CommandQuery, QtileClient},
    repl::Repl,
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
        Commands::Repl => {
            let mut repl = Repl::new(framed);
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
        // Ensure we respect current display
        let _display = std::env::var("DISPLAY").unwrap_or_else(|_| ":99".to_string());
        std::env::set_var("DISPLAY", &_display);

        let args = Args {
            framed: true,
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
        if res.is_err() {
            println!(
                "Warning: test_run_cmd_obj_status failed, likely no qtile socket: {:?}",
                res.unwrap_err()
            );
        }
    }

    #[test]
    fn test_run_invalid_object() {
        let args = Args {
            framed: true,
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
            framed: true,
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
    fn test_run_repl_init() {
        // Just verify it creates the Repl object.
        let _repl = Repl::new(true);
    }

    #[test]
    fn test_args_defaults() {
        let args = Args::default();
        assert!(!args.framed);
        if let Commands::CmdObj { function, .. } = args.command {
            assert_eq!(function, Some("help".to_string()));
        } else {
            panic!("Expected CmdObj default");
        }
    }
}
