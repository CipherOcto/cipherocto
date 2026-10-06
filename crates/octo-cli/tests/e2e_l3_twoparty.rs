//! Two-party end-to-end suite — the scenarios C3 dispositioned as
//! `multi-node` because each one needs a second party.
//!
//! ## Why these are here and not in `e2e_l3_multinode.rs`
//!
//! §10, §11 and §31 were all dispositioned "wants two holders / two peers /
//! two tenants" on the assumption that a two-party scenario needs two
//! **nodes**. That assumption was wrong, and checking it rather than
//! accepting it is the reason this file exists.
//!
//! A party is a **DID**, not a machine. `$OCTO_HOME` can hold several
//! identities (`identity register` then `identity select --did`), and
//! `capability list` is scoped to the *active* DID
//! (`commands/capability.rs` — `list_for_holder_disk(&active_did, …)`).
//! So two distinct parties are reachable inside one home, through the real
//! binary, with no docker engine. The unit under test is the *party
//! boundary*, and a party boundary does not care how many processes or
//! containers it spans.
//!
//! The consequence for the disposition table: these are `covered`, not
//! `multi-node`, and the count moves. What they do NOT prove is anything
//! about cross-process or cross-container isolation — that is the existing
//! L3/L4 suites' job, and it is unaffected.
//!
//! Every scenario below drives the operator-documented command surface, so a
//! test that passes is evidence about the guide, not about the test.

mod common;

use common::{new_node_home, octo_in, Envelope};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

/// A passphrase wide of the substrate's 12-char floor.
///
/// Written to the unlock file WITHOUT a trailing newline: the substrate
/// stores it as-is, and the stdin path trims CR/LF, so a file with a
/// newline unlocks with a passphrase the caller never typed.
const PASSPHRASE: &str = "l3-two-party-passphrase-2026";

/// Register `label` in `home` and return its canonical DID.
///
/// Dev mode throughout: the substrate's `index.json` records a single mode
/// for the issuer, and the capability mint path admits dev mode. Registering
/// in one mode and signing in another is not a combination the wallet store
/// is built to serve, so every scenario here stays in dev.
fn register_identity(home: &Path, label: &str) -> String {
    let pp_file = home.join("passphrase.txt");
    std::fs::write(&pp_file, PASSPHRASE.as_bytes()).expect("write passphrase file");

    let out = octo_in(home)
        .args([
            "identity",
            "register",
            "--label",
            label,
            "--passphrase-file",
            pp_file.to_str().expect("utf-8 passphrase path"),
            "--mode",
            "dev",
            "--allow-write",
            "--json",
        ])
        .output()
        .expect("spawn identity register");
    assert!(
        out.status.success(),
        "register `{label}` must succeed: stderr={}, stdout={}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout),
    );
    let did = Envelope::parse(&String::from_utf8_lossy(&out.stdout)).payload["did"]
        .as_str()
        .expect("register envelope must carry payload.did")
        .to_string();
    assert!(
        did.starts_with("did:octo:"),
        "DID must be in canonical wire form, got {did:?}"
    );
    did
}

/// Run a signing command with the passphrase piped on stdin.
///
/// `--passphrase-stdin` plus `--allow-stdin-secret` is the non-interactive
/// unlock path; without it every signing call exits 92 with "wallet store is
/// locked", which is a *different* failure from the one these scenarios are
/// about and would mask it.
fn signing_in(home: &Path, args: &[&str]) -> Output {
    let mut cmd = octo_in(home);
    cmd.args(args);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("spawn signing command");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(format!("{PASSPHRASE}\n").as_bytes())
        .expect("pipe passphrase");
    child.wait_with_output().expect("wait signing command")
}

/// Read the capability ids the ACTIVE identity holds.
///
/// This is the whole of the two-party question in one call: `capability list`
/// is scoped to the active DID, so "what can this party see" is answered by
/// the same command an operator would run.
fn active_capability_ids(home: &Path) -> Vec<String> {
    let out = signing_in(
        home,
        &[
            "capability",
            "list",
            "--mode",
            "dev",
            "--passphrase-stdin",
            "--allow-stdin-secret",
            "--json",
        ],
    );
    assert!(
        out.status.success(),
        "capability list must succeed on an unlocked wallet: stderr={}, stdout={}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout),
    );
    Envelope::parse(&String::from_utf8_lossy(&out.stdout)).payload["capabilities"]
        .as_array()
        .expect("payload.capabilities must be an array")
        .iter()
        .map(|c| c["cap_id"].as_str().expect("cap_id").to_string())
        .collect()
}

/// The DID `octo whoami` reports for `home`, read in a fresh process.
fn active_did(home: &Path) -> String {
    let out = Command::new(common::octo_bin())
        .args(["--mode", "dev", "whoami", "--json"])
        .env("OCTO_HOME", home)
        .env("NO_COLOR", "1")
        .output()
        .expect("spawn whoami");
    assert!(
        out.status.success(),
        "whoami must succeed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    Envelope::parse(&String::from_utf8_lossy(&out.stdout)).payload["did"]
        .as_str()
        .expect("whoami payload.did")
        .to_string()
}

// ---------------------------------------------------------------------------
// §10 — capability delivery end-to-end
// ---------------------------------------------------------------------------

/// A capability minted for one identity is visible to THAT identity and
/// invisible to the issuer.
///
/// This is the §10 lifecycle's load-bearing property, and it is the part a
/// single-identity test cannot reach. `l3_capability_mint_then_list_round_trip`
/// mints with `--holder <own did>` and then lists — which passes, and proves
/// nothing about delivery, because the issuer and the holder are the same
/// party. Here they are two.
///
/// The two-sided assertion matters. Asserting only "the buyer sees it" would
/// pass if the registry were global, which is precisely the property that must
/// NOT hold: a capability held by someone else is not the issuer's to spend.
#[test]
fn l3_capability_reaches_its_holder_and_not_the_issuer() {
    let home = new_node_home("l3-cap-two-party");
    let issuer = register_identity(&home, "issuer");
    let holder = register_identity(&home, "holder");
    assert_ne!(
        issuer, holder,
        "the two parties must be distinct identities"
    );

    // The issuer holds nothing yet.
    assert!(
        active_capability_ids(&home).is_empty(),
        "a fresh wallet must hold no capabilities"
    );

    // Mint, with the HOLDER as the named party rather than the signer.
    let minted = signing_in(
        &home,
        &[
            "capability",
            "mint",
            "--caveats",
            "[]",
            "--holder",
            &holder,
            "--mode",
            "dev",
            "--allow-write",
            "--confirm",
            "--confirm-acknowledge",
            "--passphrase-stdin",
            "--allow-stdin-secret",
            "--json",
        ],
    );
    assert_eq!(
        minted.status.code(),
        Some(0),
        "minting a capability to a second party must exit 0. stderr={}, stdout={}",
        String::from_utf8_lossy(&minted.stderr),
        String::from_utf8_lossy(&minted.stdout),
    );
    let capability_id = Envelope::parse(&String::from_utf8_lossy(&minted.stdout)).payload
        ["capability_id"]
        .as_str()
        .expect("mint envelope must carry payload.capability_id")
        .to_string();

    // Side one: the ISSUER must not see it. The registry is per-holder.
    assert!(
        active_capability_ids(&home).is_empty(),
        "the issuer must not see a capability held by another identity; \
         `capability list` is scoped to the active DID and the issuer is still active"
    );

    // Switch to the holder, in its own process.
    let select = octo_in(&home)
        .args([
            "identity",
            "select",
            "--did",
            &holder,
            "--mode",
            "dev",
            "--allow-write",
            "--json",
        ])
        .output()
        .expect("spawn identity select");
    assert!(
        select.status.success(),
        "identity select must succeed: stderr={}, stdout={}",
        String::from_utf8_lossy(&select.stderr),
        String::from_utf8_lossy(&select.stdout),
    );

    // Side two: the HOLDER must see exactly that capability.
    let holder_ids = active_capability_ids(&home);
    assert_eq!(
        holder_ids.len(),
        1,
        "the holder must see exactly the one capability minted to her, got {holder_ids:?}"
    );
    // `capability list` reports a 16-hex-char `cap_id`; the mint returns the
    // full 64-hex `capability_id`. The relationship is a prefix, and asserting
    // it is what makes this a delivery check rather than a count.
    assert!(
        capability_id.starts_with(&holder_ids[0]),
        "the listed cap_id {:?} must be a prefix of the minted capability_id {:?}",
        holder_ids[0],
        capability_id
    );

    // And the isolation is not a one-shot: switching back must hide it again.
    octo_in(&home)
        .args([
            "identity",
            "select",
            "--did",
            &issuer,
            "--mode",
            "dev",
            "--allow-write",
            "--json",
        ])
        .output()
        .expect("spawn identity select back");
    assert!(
        active_capability_ids(&home).is_empty(),
        "switching back to the issuer must hide the holder's capability again"
    );
}

// ---------------------------------------------------------------------------
// §11 — reputation + attestation
// ---------------------------------------------------------------------------

/// An attestation issued by one identity binds to a DIFFERENT identity's DID.
///
/// The substrate accepts a self-attestation — `l3_governance_attest_signing_path_succeeds`
/// attests the signer's own DID and passes — so a passing attest test does not
/// by itself establish that the subject and the signer are separable. Here the
/// subject is a second identity, and the assertion is on the *relationship*
/// between the two DIDs rather than on the command's exit code alone.
#[test]
fn l3_attestation_binds_one_identity_to_another() {
    let home = new_node_home("l3-attest-two-party");
    let attester = register_identity(&home, "attester");
    let subject = register_identity(&home, "subject");
    assert_ne!(attester, subject, "parties must differ");

    // `--evidence-hash-hex` is the pre-computed BLAKE3-256 form; the
    // `--evidence-path` form would require a real evidence file on disk and
    // hashes the same bytes.
    let out = signing_in(
        &home,
        &[
            "governance",
            "attest",
            &subject,
            "route-quality:uptime-30d",
            "--evidence-hash-hex",
            &"ab".repeat(32),
            "--mode",
            "dev",
            "--allow-write",
            "--confirm",
            "--confirm-acknowledge",
            "--passphrase-stdin",
            "--allow-stdin-secret",
            "--json",
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "attesting a second identity must exit 0. stderr={}, stdout={}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout),
    );

    let payload = Envelope::parse(&String::from_utf8_lossy(&out.stdout)).payload;
    // The receipt — not the top level — carries both parties, and asserting
    // on BOTH is the whole point: a receipt that named only the subject would
    // pass a weaker test, and one that named only the signer would prove
    // nothing about delivery.
    let receipt = &payload["receipt"];
    let subject_echoed = receipt["subject_did"]
        .as_str()
        .unwrap_or_else(|| panic!("attest receipt must carry subject_did, got {payload}"));
    let signer_echoed = receipt["signer_did"]
        .as_str()
        .unwrap_or_else(|| panic!("attest receipt must carry signer_did, got {payload}"));

    assert_eq!(
        subject_echoed, subject,
        "the attestation must bind to the subject, not to the attester"
    );
    assert_eq!(
        signer_echoed, attester,
        "the attestation must be signed by the attester, who is the active identity"
    );
    assert_ne!(
        subject_echoed, signer_echoed,
        "a two-party attestation must not collapse into a self-attestation"
    );
}

// ---------------------------------------------------------------------------
// §31 — multi-tenant / multi-identity operations
// ---------------------------------------------------------------------------

/// Two identities coexist in one `$OCTO_HOME`, and the active pointer moves
/// between them and survives into later processes.
///
/// §31's subject is explicitly one home holding several tenants
/// (`octo_wallet::set_active`), so the scenario is the operator's, not a
/// container's. The persistence leg is the part that matters: `whoami` in a
/// FRESH process must report the switched identity, which is the difference
/// between a pointer that moved and state that was written.
#[test]
fn l3_active_identity_switches_between_two_tenants_and_persists() {
    let home = new_node_home("l3-multi-identity");
    let personal = register_identity(&home, "personal");
    let org = register_identity(&home, "org");

    // Both are registered, and exactly one is active.
    let listed = octo_in(&home)
        .args(["identity", "list", "--mode", "dev", "--json"])
        .output()
        .expect("spawn identity list");
    assert!(listed.status.success());
    let records = Envelope::parse(&String::from_utf8_lossy(&listed.stdout)).payload["records"]
        .as_array()
        .expect("payload.records must be an array")
        .clone();
    assert_eq!(
        records.len(),
        2,
        "one home must list both identities, got {records:?}"
    );
    let active_count = records
        .iter()
        .filter(|r| r["active"].as_bool() == Some(true))
        .count();
    assert_eq!(
        active_count, 1,
        "exactly one identity may be active at a time, got {active_count} in {records:?}"
    );
    for want in [&personal, &org] {
        assert!(
            records
                .iter()
                .any(|r| r["did"].as_str() == Some(want.as_str())),
            "`identity list` must contain {want}"
        );
    }

    // The first registration is the active one.
    assert_eq!(active_did(&home), personal, "first register becomes active");

    // Switch, then read the pointer back from a SEPARATE process.
    for (target, expected) in [(&org, &org), (&personal, &personal)] {
        let select = octo_in(&home)
            .args([
                "identity",
                "select",
                "--did",
                target,
                "--mode",
                "dev",
                "--allow-write",
                "--json",
            ])
            .output()
            .expect("spawn identity select");
        assert!(
            select.status.success(),
            "select {target} must succeed: stderr={}",
            String::from_utf8_lossy(&select.stderr)
        );
        assert_eq!(
            active_did(&home),
            *expected,
            "a fresh `whoami` process must report the identity just selected"
        );
    }
}
