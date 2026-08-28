//! Global CLI flags — RFC-0011 §Binary Surface.

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
    /// Developer / test surface. Permits `InMemorySigner` and
    /// `IdentityKey::from_seed` paths that are normally refused in
    /// production. Required by RFC-0011 §Adversary Analysis (InMemorySigner
    /// downgrade): a developer running locally without an HSM needs a
    /// way to opt into the dev path WITHOUT making it the default.
    /// Setting `--dev` (or `--mode dev`) is the explicit signal.
    Dev,
}

/// Operator-mode + write-gating flags (global).
#[derive(Args, Debug, Clone, Default)]
pub struct OperatorModeFlags {
    /// Operator mode.
    #[arg(long, global = true, value_enum, default_value_t = OperatorMode::Human)]
    pub mode: OperatorMode,
    /// Shortcut for `--mode dev`. Enables the `InMemorySigner` /
    /// `IdentityKey::from_seed` dev paths refused by default. The
    /// `dev` flag is independent of `mode`; passing both is allowed
    /// (the result is dev semantics). Use `cli.mode()` (in
    /// `commands/mod.rs`) to read the resolved mode.
    #[arg(long, global = true)]
    pub dev: bool,
    /// Permit mutating operations.
    #[arg(long, global = true)]
    pub allow_write: bool,
    /// Confirm a mutating operation (pairs with `--confirm-acknowledge`).
    #[arg(long, global = true)]
    pub confirm: bool,
    /// Acknowledge authority delegation after reviewing the canonical
    /// payload (pastejacking defense per RFC-0011 §Security
    /// Considerations 1a). Must be passed alongside `--confirm` to
    /// authorise a Human-mode write — two explicit non-interactive
    /// flags prove the operator reviewed the echoed payload, not a
    /// pasted one-shot command. `--allow-write` alone (CI mode) does
    /// NOT require `--confirm-acknowledge` because CI scripts are
    /// trusted to supply it via the pipeline gate contract.
    /// `--dry-run` bypasses this gate: a preview grants no authority,
    /// so the second acknowledgement is irrelevant. Dev mode also
    /// bypasses `--confirm-acknowledge` (the developer is the
    /// acknowledgement) but still requires `--allow-write`.
    #[arg(long, global = true, requires = "confirm")]
    pub confirm_acknowledge: bool,
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
