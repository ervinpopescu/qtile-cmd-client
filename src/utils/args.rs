use clap::{Parser, Subcommand};

/// Qtile command client
#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
pub struct Args {
    #[command(subcommand)]
    /// Available CLI commands
    pub command: Commands,
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
        /// If no --function flag display available commands.
        ///
        /// The root node is selected by default or you can pass `root` explicitly.
        object: Option<Vec<String>>,
        #[arg(short, long, num_args = 1, default_value = "help")]
        /// Select function to execute.
        function: Option<String>,
        #[arg(short, long, num_args = 1.., default_value = None)]
        /// Set arguments supplied to function.
        args: Option<Vec<String>>,
        #[arg(short, long, num_args = 0)]
        /// With both `-o`/`--object` and ``-f``/``--function`` args prints documentation for function.
        info: bool,
    },
}
