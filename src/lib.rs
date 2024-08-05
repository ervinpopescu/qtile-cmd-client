//! I've written this qtile client in order to learn some more Rust (get used to [`serde`], [`clap`] and
//! other crates) and to get faster responses than when I used `qtile cmd-obj` and other such
//! commands (which were most likely slower because of the underlying Python bonanza).
#![warn(
    dead_code,
    unused_variables,
    unreachable_code,
    unused_imports,
    missing_docs
)]
#[cfg(test)]
pub mod tests;
/// Utilities for interacting with Qtile
pub mod utils;
