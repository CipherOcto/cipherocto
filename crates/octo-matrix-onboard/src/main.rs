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
use cli::{Cli, Command, E2eeAction, LoginMode, RecoveryAction, SessionAction};
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
                // R1-L8: `oidc::run` no longer takes a `_sso: bool`
                // (the parameter was dead — both OIDC and SSO use
                // the same code path; the binary distinguishes them
                // by logging context only). The `LoginMode::Sso`
                // arm remains so `octo-matrix-onboard login sso`
                // is still a valid subcommand; future divergence
                // can re-introduce a parameter.
                LoginMode::Oidc(args) | LoginMode::Sso(args) => modes::oidc::run(args).await,
                LoginMode::Qr(args) => modes::qr::run(args).await,
            },
            Command::Whoami(args) => whoami::run(args).await,
            Command::E2ee { action } => match action {
                E2eeAction::Bootstrap(args) => modes::e2ee::bootstrap(args).await,
                E2eeAction::Verify(args) => modes::e2ee::verify(args).await,
                E2eeAction::VerifySession(args) => modes::e2ee::verify_session(args).await,
                E2eeAction::Recovery { action } => match action {
                    RecoveryAction::Generate(args) => modes::e2ee::recovery_generate(args).await,
                    RecoveryAction::Restore(args) => modes::e2ee::recovery_restore(args).await,
                },
            },
            Command::Session { action } => match action {
                SessionAction::List(args) => modes::session::list(args).await,
                SessionAction::Use(args) => modes::session::use_(args).await,
                SessionAction::Remove(args) => modes::session::remove(args).await,
                SessionAction::Import(args) => modes::session::import(args).await,
            },
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
