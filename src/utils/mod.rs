/// CLI argument definitions using [`clap`]
pub mod args;
/// Client structs
/// - lib: [`QtileClient`](client::QtileClient)
pub mod client;
/// Qtile graph
///
/// We only use [`ObjectType`](graph::ObjectType) from this module, as of now.
pub mod graph;
/// Handles communication with the Qtile socket
pub mod ipc;
/// Translates CLI parameters into Qtile IPC JSON payloads
pub mod parser;
/// Interactive REPL mode
#[cfg(feature = "repl")]
pub mod repl;
