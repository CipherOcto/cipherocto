//! Global CLI flags — RFC-0011 §Clap Root Struct.

use clap::{Args, ValueEnum};

/// Output-shaping flags (global).
#[derive(Args, Debug, Clone, Default)]
pub struct OutputFlags {
    /// Force JSON envelope output regardless of TTY detection.
    #[arg(long, global = true)]
    pub json: bool,
    /// Disable ANSI colouring in pretty output.
    #[arg(long, global = true)]
    pub no_color: bool,
}

/// Operator execution mode.
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OperatorMode {
    /// Interactive human operator (default).
    #[default]
    Human,
    /// Non-interactive CI runner.
    Ci,
    /// Read-only auditor.
    Auditor,
}

/// Operator-mode + write-gating flags (global).
#[derive(Args, Debug, Clone, Default)]
pub struct OperatorModeFlags {
    /// Operator mode.
    #[arg(long, global = true, value_enum, default_value_t = OperatorMode::Human)]
    pub mode: OperatorMode,
    /// Permit mutating operations.
    #[arg(long, global = true)]
    pub allow_write: bool,
    /// Confirm a mutating operation (pairs with `--confirm-acknowledge`).
    #[arg(long, global = true)]
    pub confirm: bool,
    /// Preview the effect of a mutating operation without applying it.
    #[arg(long, global = true)]
    pub dry_run: bool,
    /// Permit reading a secret from stdin.
    ///
    /// Gate contract (RFC-0011 §Adversary Analysis → Stdin secret exfiltration via '--holder'): every future stdin
    /// reader must call
    /// `octo_cli::error::ensure_stdin_secret_allowed(self.allow_stdin_secret)`
    /// before consuming pipe data. Without the flag the helper returns
    /// `OctoCliError::StdinSecretRefused` (exit 15), so the refusal +
    /// exit-code contract is owned by one helper rather than scattered
    /// across every call site.
    ///
    /// The flag is currently unused: no command reads stdin in this
    /// RFC's command surface. The gate helper in `error.rs` is wired
    /// so that the first stdin reader can drop in one line and inherit
    /// the refusal semantics for free.
    #[arg(long, global = true)]
    pub allow_stdin_secret: bool,
}
