//! `octo` binary entrypoint — RFC-0011 §Binary Surface.

use clap::Parser;
use octo_cli::redact::OctoCliRedactor;
use octo_cli::{commands, Octo};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

fn main() {
    // Initialise the redacting tracing layer so secrets never reach the
    // operator's terminal. We register ONLY the redactor (no default
    // `fmt::Layer`) so the redactor is the sole writer — see `redact.rs`
    // for the design rationale. Init errors are swallowed silently:
    // a second invocation in tests legitimately fails with
    // `TryInitError`, and we don't want to pre-empt that.
    let _ = tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(OctoCliRedactor)
        .try_init();

    // `--help` / `--version` are not failures: clap renders them itself and
    // the process exits 0.
    let mut cli = match Octo::try_parse() {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };

    // RFC-0011 §NO_COLOR: disable ANSI colouring whenever the operator
    // sets the well-known `NO_COLOR` environment variable, regardless of
    // any `--no-color` flag they may have passed.
    if std::env::var_os("NO_COLOR").is_some() {
        cli.output.no_color = true;
    }

    if let Err(e) = commands::dispatch(&cli) {
        let force_json = std::env::var_os("OCTO_FORCE_JSON").is_some();
        e.render(force_json);
    }
}
