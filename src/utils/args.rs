use clap::{Parser, Subcommand};

/// qtile-cmd-client (qticc) — fast Rust replacement for `qtile cmd-obj`
#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
pub struct Args {
    #[command(subcommand)]
    /// Available CLI commands
    pub command: Commands,
    #[cfg(feature = "framing")]
    #[arg(long, global = true)]
    /// Use the new JSON IPC framing protocol (length-prefixed messages).
    pub framed: bool,
}

/// Available CLI commands
#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Access the command interface from a shell.
    ///
    /// Examples:
    ///
    ///   ```bash
    ///   qticc cmd-obj
    ///   qticc cmd-obj -o root # same as above
    ///   qticc cmd-obj -o root -f prev_layout -a 3 # prev_layout on group 3
    ///   qticc cmd-obj -o group 3 -f focus_back
    ///   qticc cmd-obj -o root -f restart # restart qtile
    ///  The graph traversal recurses:
    ///   qticc cmd-obj -o screen 0 bar bottom screen group window -f info
    ///   ```
    #[command(name = "cmd-obj")]
    CmdObj {
        #[arg(short, long, num_args = 1.., default_value=None)]
        /// Specify path to object (space separated).
        ///
        /// If no --function flag is given, available commands are listed.
        ///
        /// The root node is selected by default or you can pass `root` explicitly.
        object: Option<Vec<String>>,
        #[arg(short, long, num_args = 1, default_value = None)]
        /// Select function to execute.
        function: Option<String>,
        #[arg(short, long, num_args = 1.., default_value = None)]
        /// Set arguments supplied to function.
        args: Option<Vec<String>>,
        #[arg(short, long, num_args = 0, requires = "function")]
        /// With `-f`/`--function` prints documentation for the named function.
        info: bool,
        #[arg(short, long, num_args = 0)]
        /// Output the result as JSON.
        json: bool,
    },
    /// Start an interactive REPL session.
    #[cfg(feature = "repl")]
    Repl,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_parse_function_flag() {
        let args = Args::try_parse_from(["qticc", "cmd-obj", "-f", "status"]).unwrap();
        match args.command {
            Commands::CmdObj { function, .. } => assert_eq!(function, Some("status".into())),
            #[cfg(feature = "repl")]
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_parse_object_and_function() {
        let args =
            Args::try_parse_from(["qticc", "cmd-obj", "-o", "group", "1", "-f", "info"]).unwrap();
        match args.command {
            Commands::CmdObj {
                object, function, ..
            } => {
                assert_eq!(object, Some(vec!["group".into(), "1".into()]));
                assert_eq!(function, Some("info".into()));
            }
            #[cfg(feature = "repl")]
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_parse_info_flag() {
        let args = Args::try_parse_from(["qticc", "cmd-obj", "-f", "status", "--info"]).unwrap();
        match args.command {
            Commands::CmdObj { info, .. } => assert!(info),
            #[cfg(feature = "repl")]
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_parse_json_flag() {
        let args = Args::try_parse_from(["qticc", "cmd-obj", "-f", "status", "--json"]).unwrap();
        match args.command {
            Commands::CmdObj { json, .. } => assert!(json),
            #[cfg(feature = "repl")]
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_parse_args_flag() {
        let args =
            Args::try_parse_from(["qticc", "cmd-obj", "-f", "spawn", "-a", "xterm"]).unwrap();
        match args.command {
            Commands::CmdObj { args, .. } => assert_eq!(args, Some(vec!["xterm".into()])),
            #[cfg(feature = "repl")]
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_no_function_flag_yields_none() {
        // Without -f, function must be None so the parser shows the help listing
        let args = Args::try_parse_from(["qticc", "cmd-obj"]).unwrap();
        match args.command {
            Commands::CmdObj { function, .. } => assert_eq!(function, None),
            #[cfg(feature = "repl")]
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_info_requires_function() {
        // --info without -f must be rejected at parse time
        assert!(Args::try_parse_from(["qticc", "cmd-obj", "--info"]).is_err());
    }

    #[test]
    fn test_unknown_subcommand_fails() {
        assert!(Args::try_parse_from(["qticc", "not-a-command"]).is_err());
    }
}
