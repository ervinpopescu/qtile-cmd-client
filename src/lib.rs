//! High-performance Rust client for Qtile's IPC.
//!
//! Designed as a fast alternative to the standard Python `cmd-obj` tool,
//! leveraging Unix domain sockets and Rust's efficient serialization.
#![warn(dead_code, unused_variables, unreachable_code, unused_imports)]

#[cfg(test)]
pub mod tests;
/// Utilities for interacting with Qtile
pub mod utils;
