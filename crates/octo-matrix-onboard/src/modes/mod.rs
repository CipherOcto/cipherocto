//! Per-mode login flows.
//!
//! Each module implements a `pub async fn run(...)` for one of the
//! four login modes the CLI supports. All return
//! `Result<(), OnboardError>` so the dispatch in `main.rs` can map
//! failures to the right exit code.

pub mod e2ee;
pub mod oidc;
pub mod password;
pub mod qr;
pub mod session;
