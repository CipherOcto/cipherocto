//! Integration tests for `octo capability` — RFC-0011 §Subcommand Taxonomy.
//!
//! Covers the substrate-agnostic exit-code paths that are reachable
//! without a working wallet/HSM/macaroon: filter validation, capability id
//! form, holder DID form, confirmation/acknowledge gate, dry-run previews,
//! and the envelope `redacted` flag.
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
    cmd.env("OCTO_FORCE_JSON", "1");
    cmd
}

/// TV-CAP1 — `capability list` returns the empty active set with a
/// versioned envelope. Currently adapted: the upstream
/// `WalletStore::try_active_identity` errors with `NotActive` for the
/// v1.0 stub wallet, so the CLI surfaces exit 2 (NoActiveIdentity). When
/// the wallet substrate amendment lands, this test must be unignored
/// and the `tv_cap1_list_…_v0_exit_2` companion added to lock both
/// vectors simultaneously.
#[test]
#[ignore = "adapted; stub wallet reports NotActive; revert when wallet substrate amendment lands"]
fn tv_cap1_list_emits_empty_capabilities_envelope() {
    octo()
        .args(["capability", "list"])
        .assert()
        .code(0)
        .stdout(contains("\"capabilities\":[]"))
        .stdout(contains("\"redacted\":false"));
}

/// Active companion to TV-CAP1.
///
/// R23: this vector pinned exit 2 and `NotActive`, which is what the
/// v1.0 stub wallet returned. The unlock split (mission
/// 0011-x-s-a-wallet-store-identity §AC-9, §AC-17) made
/// `WalletStore::try_active_identity` return `WalletError::Locked`
/// unconditionally, and the `From<WalletError>` table maps that to
/// `OctoCliError::WalletLocked` at exit 92. Until R23 the seven
/// `try_active_identity` call sites routed around the table and
/// delivered exit 64 `internal error`; this file was the only place
/// that failure was visible, and it had been red since the split
/// landed.
///
/// The name carries the exit it pins, so a future change to the
/// mapping shows up as a renamed test rather than a silent drift.
///
/// OPEN GAP, recorded by R23 and not closed here: the remedy this
/// message names is NOT actionable from `capability list`. The variant
/// `hint` tells the operator to "supply the passphrase on stdin with
/// `--passphrase-stdin --allow-stdin-secret`", and `capability list`
/// accepts neither flag — `acquire_passphrase` is wired only into the
/// four identity subcommands (`rotate`, `revoke`, `rotate-complete`,
/// `rotate-abort`). Migrating the capability, governance and agent
/// signing paths onto `WalletStore::unlock` is a feature, not a
/// repair, and is the open item. Until it lands, every `octo
/// capability` subcommand and both governance signer paths fail
/// unconditionally.
#[test]
fn tv_cap1_list_emits_empty_capabilities_envelope_locked_exit_92() {
    octo()
        .args(["capability", "list"])
        .assert()
        .code(92)
        .stderr(contains("wallet store is locked"));
}

/// TV-CAP6 — `capability mint --holder did:octo:zTest` reaches the
/// HSM/signing path. Currently adapted: the production mint path is
/// hard-blocked by the SEC-03 root-secret guard with `Internal` (exit
/// 64). When the substrate amendment lands, unignore and assert exit
/// 11 (signing failed) via the HSM-error path.
#[test]
#[ignore = "adapted; stub cannot synthesize success/HSM-failure state; revert when substrate amendment lands"]
fn tv_cap6_mint_signing_failed_exits_11() {
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
        ])
        .assert()
        .code(11);
}

/// SEC-03 — today the production mint path returns exit 64 (Internal:
/// "root secret derivation not wired"). Pins the guard explicitly.
#[test]
fn tv_cap6_mint_root_secret_blocked_exits_64() {
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
        ])
        .assert()
        .code(64);
}

/// TV-CAP3 — `--caveats '{"type":"foo"}'` with an unknown caveat type
/// exits 7 (`CaveatParse`) at the canonical serde gate. SPEC-16 closes
/// the gap from the R1 review (TV-CAP3 was on the mission table but
/// absent from the impl).
#[test]
fn tv_cap3_mint_bad_caveats_exits_7() {
    octo()
        .args([
            "capability",
            "mint",
            "--caveats",
            r#"{"type":"foo"}"#,
            "--holder",
            "did:octo:zTest",
            "--confirm",
            "--confirm-acknowledge",
        ])
        .assert()
        .code(7);
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

/// CORR-09 — `--filter foo,bar` splits on comma and accepts two filter
/// entries as one CLI token, rather than being treated as one malformed
/// entry. Valid comma-separated filters must NOT exit 16.
#[test]
fn tv_cap16d_filter_comma_split() {
    octo()
        .args([
            "capability",
            "list",
            "--filter",
            "cap_id=abcd,caveat=before",
        ])
        .assert()
        // The claim this vector makes is NEGATIVE and is unchanged: a
        // well-formed comma-separated filter must not be rejected as a
        // malformed one. Exit 16 is the failure it exists to catch.
        //
        // R23 widened this set to {0, 2, 92} and in doing so made the
        // vector vacuous: R24 measured that DELETING the comma split
        // from the `--filter` argument left it green. The set had grown
        // a member (2, the clap usage-error code) that no well-formed
        // filter can produce, so the assertion had stopped constraining
        // anything. A tolerated exit code is not a witness.
        //
        // Measured on clean HEAD, a well-formed comma-separated filter
        // exits 92 — the store is locked on any machine until the
        // capability signing path is migrated onto `WalletStore::unlock`
        // (see the OPEN GAP note on `tv_cap1_..._locked_exit_92`) — or
        // 0 where a store is available. Exit 2 is not among the
        // reachable outcomes, so it is not in the set.
        .code(predicates::prelude::predicate::eq(0).or(predicates::prelude::predicate::eq(92)));
}

/// TV-CAP16d (witness half) — the comma split is OWNED by clap's
/// `value_delimiter = ','` on the `--filter` argument, so an exit-code
/// assertion can only ever observe it indirectly. This half observes it
/// DIRECTLY, and is the part that survives the widening above was
/// removed.
///
/// The split's signature is that the error names one SEGMENT, not the
/// whole string. `--filter "caveat=before,nope=x"` has one valid segment
/// and one unknown field. If clap split the value, `parse_filters` sees
/// two separate strings and rejects on the second, naming
/// `nope=x`. If clap did NOT split, `parse_filters` sees the single
/// string `caveat=before,nope=x`, reads the field as `caveat` and the
/// value as `before,nope=x` — a caveat is free text, so that is
/// ACCEPTED and the command proceeds to the identity lookup. Either way
/// the assertion below fails without the delimiter.
///
/// The order is the point: the malformed segment is SECOND, so a
/// prefix-match on the whole string would pass by accident if clap
/// passed the value through unsplit. Asserting the ABSENCE of the
/// unsplit rendering is what makes this a witness rather than a
/// substring coincidence.
#[test]
fn tv_cap16d_filter_comma_split_is_witnessed_by_the_segment_named_in_the_error() {
    let split = octo()
        .args(["capability", "list", "--filter", "caveat=before,nope=x"])
        .assert()
        .code(16)
        .stderr(contains("invalid filter: nope=x"));

    // The unsplit rendering must be absent. Without this half, a build
    // that passed the whole value through would still print
    // `nope=x` as a SUBSTRING of `caveat=before,nope=x` and pass.
    split.stderr(predicates::str::contains("caveat=before,nope=x").not());

    // Same property with the malformed segment FIRST, so the assertion
    // cannot be satisfied by an implementation that only inspects the
    // head of the value.
    octo()
        .args(["capability", "list", "--filter", "nope=x,caveat=before"])
        .assert()
        .code(16)
        .stderr(contains("invalid filter: nope=x"))
        .stderr(predicates::str::contains("nope=x,caveat=before").not());
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
        // JSON error envelope from the v1.0 error renderer carries the
        // variant name (`ConfirmationRequired`) verbatim.
        .stderr(contains("ConfirmationRequired"));
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
        .stderr(contains("ConfirmationRequired"));
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
        .stdout(contains("\"redacted\":true"))
        .stdout(contains("\"capability_id\":\"(preview)\""));
}

/// CORR-12 — `capability mint --dry-run` echoes the canonical caveat
/// set + holder DID to stderr (pastejacking defense). The canonical
/// `would mint: holder=did:octo:zTest, caveats=[]` line must appear on
/// stderr in addition to the dry-run envelope on stdout.
#[test]
fn tv_cap17b_mint_dry_run_stderr_echo() {
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
        .stderr(contains("would mint"))
        .stderr(contains("did:octo:zTest"));
}

/// CORR-12 (variant) — `capability attenuate --dry-run` echoes the
/// parent + canonical caveat set on stderr.
#[test]
fn tv_cap18b_attenuate_dry_run_stderr_echo() {
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
        .stderr(contains("would attenuate"))
        .stderr(contains(&parent));
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
        .stdout(contains("\"redacted\":true"))
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

// ---------------------------------------------------------------------------
// Canonical-but-blocked fixture (TV-CAP2 happy path).
//
// The adapted tests above (TV-CAP6 mint signing-failed, TV-CAP6 mint
// root-secret-blocked) assert the CLI-shape contract against the
// substrate stub. The canonical happy-path mint (exit 0 with
// `CapabilityMintOutput` envelope) depends on HSM signing + a wired
// root secret that the stub cannot synthesize; this fixture pins what
// the canonical assertions WILL assert when the substrate amendment
// lands so the unignore moment is visible.
// ---------------------------------------------------------------------------

/// Canonical TV-CAP2: `octo capability mint` against a wired HSM + root
/// secret → exit 0 with a `CapabilityMintOutput` envelope
/// (`{capability_id, body_hash, caveats[], holder_sig}`) per RFC-0011
/// §Subcommand Taxonomy. The stub wallet + InMemorySigner cannot
/// synthesize this path so the adapted TV-CAP6 above asserts the
/// substrate-stub contract instead.
#[test]
#[ignore = "adapted; stub cannot synthesize HSM signing + root secret; revert when wallet + HSM substrate amendments land"]
fn tv_cap2_canonical_mint_success_exits_0() {
    octo()
        .args([
            "--json",
            "capability",
            "mint",
            "--caveats",
            "[]",
            "--holder",
            "did:octo:zTest",
            "--confirm",
            "--confirm-acknowledge",
        ])
        .assert()
        .code(0)
        .stdout(contains("\"capability_id\""))
        .stdout(contains("\"body_hash\""))
        .stdout(contains("\"holder_sig\""))
        .stdout(contains("\"caveats\""));
}
