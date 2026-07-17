---
name: rust-ci-check
description: Run the full Rust quality gate (cargo fmt, clippy, test) for one or more crates. Use before every commit, after code changes, or when the user asks to "check", "lint", "test", or "CI" a crate. Handles package discovery, feature flags, and standard flags automatically.
keywords: cargo, fmt, clippy, test, ci, lint, check, rust, quality gate, before commit
---

# Rust CI Check

Runs the standard 3-step quality gate for Rust crates in this project:
1. `cargo fmt` — format check
2. `cargo clippy` — lint check
3. `cargo test` — unit/integration tests

## Quick Usage

```bash
# Single crate (most common)
rust-ci-check <crate-name>

# Multiple crates
rust-ci-check <crate1> <crate2> ...

# With specific features
rust-ci-check <crate-name> --features "feature1,feature2"

# Test only (skip fmt+clippy)
rust-ci-check <crate-name> --test-only
```

## Full Procedure

### Step 1: Format check

```bash
cargo fmt -- --check 2>&1 | tail -20
```

If formatting issues exist, auto-fix them:
```bash
cargo fmt
```

### Step 2: Clippy lint

For workspace crates, use `-p <crate>` with `--all-targets --all-features`:

```bash
# Standard crate
cargo clippy -p <crate> --all-targets --all-features -- -D warnings 2>&1 | tail -30

# Crate with specific feature flags (e.g. real-tdlib)
cargo clippy -p <crate> --features real-tdlib -- -D warnings 2>&1 | tail -20

# Multiple crates at once
cargo clippy -p <crate1> -p <crate2> --all-targets --all-features -- -D warnings 2>&1 | tail -30
```

### Step 3: Tests

```bash
# Library tests
cargo test -p <crate> --lib 2>&1 | tail -20

# All tests (including integration)
cargo test -p <crate> 2>&1 | tail -40

# With specific features
cargo test -p <crate> --features real-tdlib 2>&1 | tail -40
```

## Common Crate Profiles

These are the frequently-tested crates in this project:

| Crate | Features | Notes |
|-------|----------|-------|
| `octo-adapter-telegram` | `real-tdlib` | TDLib adapter; test with `--features real-tdlib` |
| `quota-router-core` | `full` | Quota router; use `--features full` |
| `octo-adapter-matrix-sdk` | (default) | Matrix adapter |
| `octo-matrix-onboard` | (default) | Matrix onboarding CLI |
| `octo-matrix-onboard-core` | (default) | Matrix onboarding core lib |
| `octo-matrix-session-store` | (default) | Matrix session store |

## Project Rules

From MEMORY.md:
- **Always run `cargo fmt -- --check` before every commit** (CRITICAL RULE #3)
- Use `-D warnings` with clippy to treat warnings as errors
- Pipe output through `tail -20` or `tail -40` to keep output readable

## Error Handling

- If `cargo fmt` fails → run `cargo fmt` (no `--check`) to auto-fix, then re-check
- If clippy reports warnings → they are treated as errors (`-D warnings`); fix each one
- If tests fail → report the failing test name and output; do not auto-fix test logic

## Automation Note

This skill is designed for manual invocation. The agent should run it:
1. After completing code changes (before asking user to commit)
2. When the user says "check", "lint", "test", "CI", or "cargo check"
3. As part of the pre-commit workflow
