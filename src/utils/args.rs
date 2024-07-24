use clap::{Parser, Subcommand};

/// Command line arguments
#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
pub struct Args {
    #[command(subcommand)]
    pub command: Commands,
}
#[derive(Subcommand, Debug, Clone)]
pub(crate) enum Commands {
    /// Access the command interface from a shell.
    #[command(name = "cmd-obj")]
    CmdObj {
        /// Specify path to object (space separated).
        ///
        /// If no --function flag display available commands.
        ///
        /// The root node is selected by default or you can pass `root` explicitly.
        #[arg(short, long, num_args = 1.., default_value=None)]
        object: Option<Vec<String>>,
        /// Select function to execute.
        #[arg(short, long, num_args = 1, default_value = "help")]
        function: Option<String>,
        #[arg(short, long, num_args = 1.., default_value = None)]
        args: Option<Vec<String>>,
        #[arg(short, long, num_args = 0)]
        info: bool,
    },
}
