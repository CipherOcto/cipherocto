//! `octo-matrix-onboard` — CLI binary for Matrix homeserver onboarding.
//!
//! Mission 0850h-a. See `docs/plans/2026-06-02-matrix-auth-onboarding-design.md`
//! for the full design.

mod cli;
mod error;
mod logging;
mod modes;
mod output;
mod whoami;

use clap::Parser;
use cli::{Cli, Command, LoginMode};
use error::OnboardError;
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    logging::init(cli.verbose);

    let result: Result<(), OnboardError> = async {
        match cli.command {
            Command::Login { mode } => match mode {
                LoginMode::Password(args) => modes::password::run(args).await,
                LoginMode::Oidc(args) => modes::oidc::run(args, false).await,
                LoginMode::Sso(args) => modes::oidc::run(args, true).await,
                LoginMode::Qr(args) => modes::qr::run(args).await,
            },
            Command::Whoami(args) => whoami::run(args).await,
            Command::Version => {
                println!("octo-matrix-onboard {}", env!("CARGO_PKG_VERSION"));
                Ok(())
            }
        }
    }
    .await;

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!("{:#}", e);
            e.as_exit_code()
        }
    }
}
