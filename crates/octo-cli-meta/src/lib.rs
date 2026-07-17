//! Meta-crate for CLI tools.
//!
//! This crate exists purely as a feature-flag holder. It has no library API,
//! no binaries, no tests. Its purpose is to provide a stable target for
//! `cargo build -p octo-cli-meta --features <cli-name>` from the repo root.
//!
//! ## Why this exists
//!
//! Some CLIs pull in heavy native dependencies (e.g., the Telegram CLI uses
//! TDLib, a ~150 MB C++ library, plus its `libc++` runtime). If those CLI
//! crates were default workspace members, cargo's feature unification would
//! activate their heavy deps for every workspace member, including the test
//! binaries that CI runs.
//!
//! The fix is twofold:
//!
//! 1. The CLI crates are **excluded** from the main workspace (see root
//!    `Cargo.toml` `[workspace] exclude` list). `cargo test --workspace` and
//!    `cargo build --workspace` from the root never build them.
//!
//! 2. This meta-crate is a workspace member with optional dependencies on the
//!    CLI crates, gated behind feature flags. CI never enables any feature,
//!    so the optional deps are never built. Operators who want a CLI build
//!    it explicitly:
//!
//!    ```sh
//!    cargo build -p octo-cli-meta --features telegram-cli
//!    ```
//!
//! ## Adding a new CLI
//!
//! 1. Add a new feature and optional dependency in `Cargo.toml`.
//! 2. Add the CLI crate to the root `Cargo.toml` `[workspace] exclude` list.
//! 3. Document the build command in this crate's `README.md`.

#![doc = ""]
