/// CLI argument parser using [`clap`]
pub mod args;
/// Client structs
/// - bin: [`ShellClient`](client::ShellClient)
/// - lib: [`InteractiveCommandClient`](client::InteractiveCommandClient)
pub mod client;
/// Qtile graph
///
/// We only use [`ObjectType`](graph::ObjectType) from this module, as of now.
pub mod graph;
/// Handles communication with the Qtile socket
pub mod ipc;
/// CommandParser (from CLI or directly from parameters)
pub mod parser;
