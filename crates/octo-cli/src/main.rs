//! `octo` binary entrypoint — RFC-0011 §Binary Surface.

use clap::Parser;
use octo_cli::flags::OperatorMode;
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

    // Disable ANSI colouring whenever the operator sets --no-color OR
    // the environment variable OCTO_FORCE_JSON is set (TTY-aware output).
    if std::env::var_os("NO_COLOR").is_some() {
        cli.output.no_color = true;
    }

    // R16 Lens-1 F4: RFC-0011 §Roles and Authorities auto-detect operator
    // mode from environment when `--mode` is not explicit. `OCTO_AUDIT=1`
    // triggers Auditor (read-only); `CI=true` triggers Ci (CI-bot). Both
    // only override the clap default (Human) when the operator did not
    // pass `--mode` explicitly.
    if matches!(cli.mode.mode, OperatorMode::Human) {
        if std::env::var_os("OCTO_AUDIT")
            .filter(|v| v == "1")
            .is_some()
        {
            cli.mode.mode = OperatorMode::Auditor;
        } else if std::env::var_os("CI").filter(|v| v == "true").is_some() {
            cli.mode.mode = OperatorMode::Ci;
        }
    }

    // R16 Lens-1 F3: previously only the error branch honoured
    // `OCTO_FORCE_JSON`; the success envelope read only `cli.output.json`.
    // Hoist the env-var OR into `cli.output.json` so both paths use the
    // same source. No `force_json` shadowing needed downstream.
    if std::env::var_os("OCTO_FORCE_JSON").is_some() {
        cli.output.json = true;
    }
    let force_json = cli.output.json;

    if let Err(e) = commands::dispatch(&cli) {
        e.render(force_json);
    }
}
