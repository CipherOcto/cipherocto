//! Library surface for the `octo-telegram-onboard` CLI binary.
//!
//! This crate exists as both a library (this file) and a binary
//! (`src/main.rs`). The library exposes the CLI's internal modules so that
//! other crates (e.g. the `octo-cli-meta` meta-crate) can depend on this
//! crate via a path dependency. Without a lib target, cargo would reject
//! the dependency ("missing a lib target").
//!
//! The binary is a thin wrapper that:
//! 1. Parses CLI args via [`cli::Cli`]
//! 2. Initializes logging via [`logging::init`]
//! 3. Dispatches to one of the `run_*` functions in `main.rs`
//!
//! No business logic lives in `main.rs` itself — it's all in the modules
//! below and in `octo-telegram-onboard-core`.

pub mod cli;
pub mod logging;
