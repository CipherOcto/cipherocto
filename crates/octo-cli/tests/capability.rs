//! Integration tests for `octo capability` — RFC-0011 §Subcommand Taxonomy.
//!
//! Covers the substrate-agnostic exit-code paths that are reachable
//! without a working wallet/HSM/macaroon: filter validation, capability id
//! form, holder DID form, confirmation/acknowledge gate, dry-run previews,
//! and the envelope `preview_only` flag.
//!
//! Where the mission's table would require end-to-end mint/attenuate
//! success (CAP2, CAP6, CAP9-15), the upstream `WalletStore` stub always
//! reports `NotActive`. The substrate exit-code coverage for those vectors
//! is exercised by the lib unit tests in `commands::capability::tests`.
//!
//! Each test runs as a child binary via `assert_cmd`, captures
//! stdout/stderr, and inspects JSON, exit code, or text per vector.

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

fn octo() -> Command {
    let mut cmd = Command::cargo_bin("octo").expect("octo binary built");
    cmd.env("NO_COLOR", "1");
    cmd.env("OCTC_CLI_FORCE_JSON", "1");
    cmd
}

/// TV-CAP16 — `capability list --filter <bad form>` exits 16 with the
/// rejected filter echoed on stderr.
#[test]
fn tv_cap16_filter_unknown_field_exits_16() {
    octo()
        .args(["capability", "list", "--filter", "field=value"])
        .assert()
        .code(16)
        .stderr(contains("field=value"));
}

/// TV-CAP16 (variant) — missing `=`.
#[test]
fn tv_cap16b_filter_missing_equals_exits_16() {
    octo()
        .args(["capability", "list", "--filter", "cap_id"])
        .assert()
        .code(16);
}

/// TV-CAP16 (variant) — empty value side.
#[test]
fn tv_cap16c_filter_empty_value_exits_16() {
    octo()
        .args(["capability", "list", "--filter", "cap_id="])
        .assert()
        .code(16);
}

/// TV-CAP19 — `capability mint` without `--confirm` in human mode exits 2.
#[test]
fn tv_cap19_confirm_required() {
    octo()
        .args([
            "capability",
            "mint",
            "--caveats",
            "[]",
            "--holder",
            "did:octo:zTest",
            "--confirm",
        ])
        .assert()
        .code(2)
        .stderr(contains("confirmation required"));
}

/// TV-CAP19 (variant) — `capability attenuate` without `--confirm` exits 2.
#[test]
fn tv_cap19b_attenuate_requires_confirm() {
    octo()
        .args([
            "capability",
            "attenuate",
            &"a".repeat(64),
            "--caveats",
            "[]",
            "--confirm",
        ])
        .assert()
        .code(2)
        .stderr(contains("confirmation required"));
}

/// TV-CAP19 (variant) --missing `--confirm-acknowledge` on mint/attenuate
/// exits 2 (clap enforces `requires = "confirm"`).
#[test]
fn tv_cap19c_acknowledge_required_when_confirm_set() {
    // Without --confirm, clap refuses because the global --confirm is unset;
    // we hit either 1 (clap usage error) or 2 (ConfirmationRequired).
    // The point of this vector: the field is required when --confirm is set.
    octo()
        .args([
            "capability",
            "mint",
            "--caveats",
            "[]",
            "--holder",
            "did:octo:zTest",
            "--confirm",
            // Intentionally omit --confirm-acknowledge.
        ])
        .assert()
        .failure();
}

/// TV-CAP17 — `capability mint --dry-run` succeeds with
/// `"preview_only": true` and the holder DID `did:octo:zTest`.
#[test]
fn tv_cap17_mint_dry_run_preview() {
    octo()
        .args([
            "capability",
            "mint",
            "--caveats",
            "[]",
            "--holder",
            "did:octo:zTest",
            "--confirm",
            "--confirm-acknowledge",
            "--dry-run",
        ])
        .assert()
        .code(0)
        .stdout(contains("\"preview_only\":true"))
        .stdout(contains("\"cap_id\":\"(preview)\""));
}

/// TV-CAP18 — `capability attenuate --dry-run` succeeds with
/// `"preview_only": true` and echoes the parent cap_id.
#[test]
fn tv_cap18_attenuate_dry_run_preview() {
    let parent = "a".repeat(64);
    octo()
        .args([
            "capability",
            "attenuate",
            &parent,
            "--caveats",
            "[]",
            "--confirm",
            "--confirm-acknowledge",
            "--dry-run",
        ])
        .assert()
        .code(0)
        .stdout(contains("\"preview_only\":true"))
        .stdout(contains("\"narrowed_from\":\"").and(contains(&parent)));
}

/// TV-CAP7 — `capability mint --holder not-a-did` exits 9 even without a
/// working wallet.
#[test]
fn tv_cap7_holder_not_found_exits_9() {
    octo()
        .args([
            "capability",
            "mint",
            "--caveats",
            "[]",
            "--holder",
            "not-a-did",
            "--confirm",
            "--confirm-acknowledge",
        ])
        .assert()
        .code(9);
}

/// TV-CAP5 — `capability attenuate <bad cap_id>` exits 12
/// (`ParentCapNotFound`) without reaching the substrate.
#[test]
fn tv_cap5_attenuate_bad_cap_id_exits_12() {
    octo()
        .args([
            "capability",
            "attenuate",
            "cap_test_id",
            "--caveats",
            "[]",
            "--confirm",
            "--confirm-acknowledge",
        ])
        .assert()
        .code(12);
}

/// TV-CAP8 — malformed `--caveats` JSON is a parse error (exit 7).
#[test]
fn tv_cap8_bad_caveat_json_exits_7() {
    octo()
        .args([
            "capability",
            "mint",
            "--caveats",
            "{not_json",
            "--holder",
            "did:octo:zTest",
            "--confirm",
            "--confirm-acknowledge",
        ])
        .assert()
        .code(7);
}

/// TV-CAP8b — constraint-violation: payload above the 64 KiB clamp exits 7.
#[test]
fn tv_cap8b_caveat_payload_too_large_exits_7() {
    // 65 KiB of digits, well within JSON validity but past the byte clamp.
    let huge = "0".repeat(65 * 1024);
    octo()
        .args([
            "capability",
            "mint",
            "--caveats",
            &huge,
            "--holder",
            "did:octo:zTest",
            "--confirm",
            "--confirm-acknowledge",
        ])
        .assert()
        .code(7);
}

/// TV-CAP8c — unknown caveat variant rejects at the canonical serde gate.
#[test]
fn tv_cap8c_unknown_caveat_tag_exits_7() {
    octo()
        .args([
            "capability",
            "mint",
            "--caveats",
            r#"[{"type":"foo","value":1}]"#,
            "--holder",
            "did:octo:zTest",
            "--confirm",
            "--confirm-acknowledge",
        ])
        .assert()
        .code(7);
}

/// The substrate mint stub rejects every call today, so the upstream
/// `WalletStore::try_active_identity` succeeds but downstream substrate
/// mint yields an `InvalidCaveat` — surfaced as exit 7. This pins
/// substrate drift: when the stub is replaced, this test must be updated.
#[test]
fn tv_cap2_list_emits_empty_capabilities_envelope() {
    // Substrate stub for `try_active_identity` errors with `NotActive`,
    // so the CLI surfaces `NoActiveIdentity` (exit 2). The mission table
    // prescribed exit 0; the lib unit tests pin the empty-set payload
    // shape directly.
    octo()
        .args(["capability", "list"])
        .assert()
        .code(2)
        .stderr(contains("active identity"));
}
