//! Mission `0957-phase1-fixture-author` — Phase 1 Test Vector Fixture
//! gate for RFC-0009.
//!
//! 3 byte-exact test vectors (TV-1..TV-3) gating Phase 1 acceptance:
//!
//! - **TV-1** — `phase1_tv_json_v11_round_trip_equivalence`:
//!   `serde_json` round-trip on `MissionId` (the v1.1 wire form that
//!   v1.2 inherits unchanged) — `to_string ∘ from_str ∘ to_string`
//!   produces byte-exact equivalence.
//! - **TV-2** — `phase1_tv_json_child_unlinkability`: 2 sibling
//!   `MissionKey` records derived via HKDF-BLAKE3 from a single
//!   `KeyHierarchy` seed + distinct `MissionId` produce 64
//!   concatenation bytes that differ bytewise AND have no shared
//!   16-byte prefix (per RFC-0009 §Hierarchical Attenuation).
//! - **TV-3** — `phase1_tv_json_hsm_boundary_no_seed_exfil`: an
//!   `InMemorySigner` produces a 64-byte Ed25519 signature; the
//!   returned signature bytes do NOT contain the seed prefix
//!   (no exfiltration via sig channel), AND the `Debug` impl
//!   redacts the seed field (no exfiltration via log/dbg
//!   channel). Per RFC-0009 §Security §Key Handling Rule 3 +
//!   mission `0009-a` A9 mitigation.
//!
//! ## Verification pattern
//!
//! Run the gate tests (loads fixture, asserts byte-exact):
//!
//! ```bash
//! cargo test -p octo-wallet --lib phase1_tv_json
//! ```
//!
//! Bootstrap the fixture (deterministic; overwrites the JSON when
//! run with `UPDATE_PHASE1_TV=1`):
//!
//! ```bash
//! UPDATE_PHASE1_TV=1 cargo test -p octo-wallet --lib phase1_tv_json -- --nocapture
//! ```
//!
//! Per [[feedback-no-fabricated-commit-rule]] + RFC-0008 Class A
//! determinism: the fixture is byte-exact reproducible from the
//! declared inputs; any drift is a substrate change (RFC + test
//! together).
//!
//! ## Out of scope (NOT this fixture)
//!
//! - Phase 3 TV-4..TV-7 (capability token cross-version, MPC
//!   threshold aggregation, ZK capability bundle) — separate
//!   fixture + mission per RFC-0009 §Test Vectors.

#![allow(clippy::vec_init_then_push)]

use std::fmt::Write;

use crate::hsm::{HsmAdapter, InMemorySigner};
use crate::key_hierarchy::{KeyHierarchy, MissionId};

/// Path to the JSON fixture, relative to the repo root.
///
/// `cargo test -p octo-wallet --lib` runs with CWD =
/// `crates/octo-wallet/`, so the fixture at the repo root is
/// `../../tests/fixtures/phase1_tv.json`.
const FIXTURE_PATH: &str = "../../tests/fixtures/phase1_tv.json";

/// TV-1: `MissionId` serde_json round-trip equivalence (v1.1 wire
/// form unchanged in v1.2).
///
/// The `MissionId` struct was introduced in RFC-0009 with
/// `serde::Serialize + Deserialize` derives. RFC-0009 inherits
/// the same wire form unchanged. This TV gates the invariant:
/// `to_string(m) == to_string(from_str(to_string(m)))` byte-exact.
///
/// Inputs:
/// - `asker_did = "did:octo:0001"`
/// - `model     = "openai/gpt-4"`
///
/// Output: serde_json canonical JSON bytes (UTF-8).
fn tv1_v11_round_trip_equivalence() -> Vec<u8> {
    let mission_id = MissionId {
        asker_did: "did:octo:0001".to_owned(),
        model: "openai/gpt-4".to_owned(),
    };
    let json = serde_json::to_string(&mission_id).expect("MissionId to_string");
    // Round-trip: deserialize then re-serialize MUST produce byte-exact equivalence.
    let roundtrip: MissionId = serde_json::from_str(&json).expect("MissionId from_str");
    let json2 = serde_json::to_string(&roundtrip).expect("roundtrip to_string");
    assert_eq!(
        json, json2,
        "v1.1 round-trip equivalence broken — to_string ≠ to_string ∘ from_str"
    );
    json.into_bytes()
}

/// TV-2: child key unlinkability (sibling MissionKeys distinct).
///
/// Two `MissionKey` records derived via HKDF-BLAKE3 from a single
/// `KeyHierarchy` seed + distinct `MissionId`s produce 64
/// concatenation bytes that differ bytewise AND have no shared
/// 16-byte prefix. Per RFC-0009 §Hierarchical Attenuation:
/// child keys MUST be cryptographically independent (unlinkable
/// across siblings).
///
/// Inputs:
/// - `seed          = [0..=31]` (32-byte monotonic identity seed)
/// - `asker_did_a   = "did:octo:a"`
/// - `model_a       = "openai/gpt-4"`
/// - `asker_did_b   = "did:octo:b"`
/// - `model_b       = "anthropic/claude-3"`
///
/// Output: 64 bytes = `mission_key_a[32] || mission_key_b[32]`.
fn tv2_child_unlinkability() -> Vec<u8> {
    let seed: [u8; 32] = [
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
        25, 26, 27, 28, 29, 30, 31,
    ];
    let h = KeyHierarchy::new(seed);
    let m_a = MissionId {
        asker_did: "did:octo:a".to_owned(),
        model: "openai/gpt-4".to_owned(),
    };
    let m_b = MissionId {
        asker_did: "did:octo:b".to_owned(),
        model: "anthropic/claude-3".to_owned(),
    };
    let k_a = h.derive_mission_key(&m_a).expect("derive m_a");
    let k_b = h.derive_mission_key(&m_b).expect("derive m_b");

    // Unlinkability invariants: siblings MUST be cryptographically independent.
    assert_ne!(
        k_a.as_bytes(),
        k_b.as_bytes(),
        "sibling MissionKeys collide — HKDF derivation broken"
    );
    assert_ne!(
        &k_a.as_bytes()[..16],
        &k_b.as_bytes()[..16],
        "shared 16-byte prefix between siblings — HKDF context overlap"
    );

    let mut out = Vec::with_capacity(64);
    out.extend_from_slice(k_a.as_bytes());
    out.extend_from_slice(k_b.as_bytes());
    out
}

/// TV-3: HSM boundary — signature does NOT leak the seed; Debug
/// redacts the seed field.
///
/// `InMemorySigner::sign(msg)` returns a 64-byte Ed25519 signature.
/// The signature bytes MUST NOT contain the seed prefix (no
/// exfiltration via the sig channel). The `Debug` impl MUST
/// redact the seed field (no exfiltration via log/`dbg!` channel).
/// Per RFC-0009 §Security §Key Handling Rule 3 + mission
/// `0009-a` A9 mitigation.
///
/// Inputs:
/// - `seed_bytes  = [0xA0; 32]` (high-entropy marker for exfil detection)
/// - `public_key  = [0xB0; 32]`
/// - `msg         = b"cipherocto/test-vector-3-hsm-boundary"` (32 bytes)
///
/// Output: 64-byte Ed25519 signature.
fn tv3_hsm_boundary_no_seed_exfil() -> Vec<u8> {
    let seed: [u8; 32] = [0xA0; 32];
    let public_key: [u8; 32] = [0xB0; 32];
    let signer = InMemorySigner::new(seed, public_key);
    let msg: &[u8] = b"cipherocto/test-vector-3-hsm-boundary";
    let sig: [u8; 64] = signer.sign(msg).expect("sign");

    // Invariant 1: signature MUST NOT contain the seed prefix.
    assert!(
        !sig.windows(seed.len()).any(|w| w == seed),
        "signature leaks 32-byte seed (sig windows match seed bytes)"
    );
    assert!(
        !sig.windows(8).any(|w| w == &seed[..8]),
        "signature leaks 8-byte seed prefix"
    );

    // Invariant 2: Debug MUST redact the seed field (no exfil via log/dbg).
    let dbg = format!("{signer:?}");
    assert!(dbg.contains("REDACTED"), "Debug leaks seed: {dbg}");
    assert!(
        !dbg.contains(&hex::encode(seed)),
        "Debug leaks seed hex: {dbg}"
    );

    sig.to_vec()
}

/// Canonical 3-TV fixture struct. JSON serialization is hand-rolled
/// (no `serde_json` dep in this test module's emit path beyond
/// `to_string`) to keep the fixture diff-friendly.
struct Phase1Fixture {
    entries: Vec<Phase1TvEntry>,
}

struct Phase1TvEntry {
    name: String,
    description: String,
    inputs_hex: Vec<(String, String)>,
    outputs_hex: String,
    byte_len: usize,
    verification_command: String,
}

impl Phase1Fixture {
    fn compute() -> Self {
        let mut entries = Vec::new();
        entries.push(Phase1TvEntry {
            name: "TV-1".to_string(),
            description:
                "MissionId serde_json round-trip equivalence (v1.1 wire form unchanged in v1.2) — \
                to_string ∘ from_str ∘ to_string produces byte-exact equivalence."
                    .to_string(),
            inputs_hex: vec![
                (
                    "asker_did_utf8".to_string(),
                    hex::encode("did:octo:0001".as_bytes()),
                ),
                (
                    "model_utf8".to_string(),
                    hex::encode("openai/gpt-4".as_bytes()),
                ),
            ],
            outputs_hex: hex::encode(tv1_v11_round_trip_equivalence()),
            byte_len: 0, // filled below from actual output
            verification_command: "cargo test -p octo-wallet --lib phase1_tv_json \
                phase1_tv_json_v11_round_trip_equivalence -- --nocapture"
                .to_string(),
        });
        let tv1_len = entries.last().unwrap().outputs_hex.len() / 2;
        entries.last_mut().unwrap().byte_len = tv1_len;

        entries.push(Phase1TvEntry {
            name: "TV-2".to_string(),
            description:
                "Child key unlinkability: 2 sibling MissionKeys derived via HKDF-BLAKE3 from a \
                single KeyHierarchy seed + distinct MissionIds produce 64 concatenation bytes that \
                differ bytewise AND have no shared 16-byte prefix."
                    .to_string(),
            inputs_hex: vec![(
                "identity_seed".to_string(),
                hex::encode([
                    0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21,
                    22, 23, 24, 25, 26, 27, 28, 29, 30, 31,
                ]),
            )],
            outputs_hex: hex::encode(tv2_child_unlinkability()),
            byte_len: 64,
            verification_command: "cargo test -p octo-wallet --lib phase1_tv_json \
                phase1_tv_json_child_unlinkability -- --nocapture"
                .to_string(),
        });

        entries.push(Phase1TvEntry {
            name: "TV-3".to_string(),
            description:
                "HSM boundary no seed exfiltration: InMemorySigner.sign(msg) returns 64-byte \
                Ed25519 signature; signature bytes do NOT contain the seed prefix (no exfil via \
                sig channel), AND Debug impl redacts the seed field (no exfil via log/dbg \
                channel)."
                    .to_string(),
            inputs_hex: vec![
                ("seed_bytes".to_string(), hex::encode([0xA0u8; 32])),
                ("public_key".to_string(), hex::encode([0xB0u8; 32])),
                (
                    "msg_utf8".to_string(),
                    hex::encode(b"cipherocto/test-vector-3-hsm-boundary"),
                ),
            ],
            outputs_hex: hex::encode(tv3_hsm_boundary_no_seed_exfil()),
            byte_len: 64,
            verification_command: "cargo test -p octo-wallet --lib phase1_tv_json \
                phase1_tv_json_hsm_boundary_no_seed_exfil -- --nocapture"
                .to_string(),
        });
        Self { entries }
    }

    fn to_json(&self) -> String {
        let mut s = String::new();
        writeln!(s).unwrap();
        s.push_str(
            "  \"_comment\": \"RFC-0009 Phase 1 test vectors (TV-1..TV-3). \
            Regenerate via `UPDATE_PHASE1_TV=1 cargo test -p octo-wallet --lib phase1_tv_json \
            phase1_tv_json_dump -- --nocapture`. Per RFC-0008 Class A determinism: every output \
            is byte-exact reproducible from the declared inputs.\",\n",
        );
        s.push_str("  \"_rfc\": \"RFC-0009\",\n");
        s.push_str("  \"_phase\": \"Phase 1\",\n");
        s.push_str(
            "  \"_scope\": \"TV-1..TV-3 ONLY. Phase 3 TV-4..TV-7 land in a separate \
            fixture + mission per RFC-0009 §Test Vectors.\",\n",
        );
        s.push_str("  \"entries\": [\n");
        for (i, e) in self.entries.iter().enumerate() {
            s.push_str("    {\n");
            writeln!(s, "      \"name\": \"{}\",", e.name).unwrap();
            writeln!(s, "      \"description\": \"{}\",", e.description).unwrap();
            s.push_str("      \"inputs\": {\n");
            for (j, (k, v)) in e.inputs_hex.iter().enumerate() {
                let comma = if j + 1 < e.inputs_hex.len() { "," } else { "" };
                writeln!(s, "        \"{k}\": \"{v}\"{comma}").unwrap();
            }
            s.push_str("      },\n");
            writeln!(s, "      \"outputs_hex\": \"{}\",", e.outputs_hex).unwrap();
            writeln!(s, "      \"byte_len\": {},", e.byte_len).unwrap();
            writeln!(
                s,
                "      \"verification_command\": \"{}\"",
                e.verification_command
            )
            .unwrap();
            let comma = if i + 1 < self.entries.len() { "," } else { "" };
            writeln!(s, "    }}{comma}").unwrap();
        }
        s.push_str("  ]\n");
        s.push_str("}\n");
        s
    }

    /// Parse the JSON fixture, returning the 3 entries. Hand-rolled
    /// minimal parser (no `serde_json` parse dep beyond `from_str`).
    fn from_json(json: &str) -> Result<Vec<Phase1TvEntry>, String> {
        let entries_start = json
            .find("\"entries\": [")
            .ok_or_else(|| "missing entries key".to_string())?;
        let after_array = entries_start + "\"entries\": [".len();
        let entries_end_rel = json[after_array..]
            .find("]\n")
            .ok_or_else(|| "missing entries closing".to_string())?;
        let entries_block = &json[after_array..after_array + entries_end_rel];

        let mut entries = Vec::new();
        let mut depth = 0_i32;
        let mut obj_start: Option<usize> = None;
        for (i, ch) in entries_block.char_indices() {
            match ch {
                '{' => {
                    if depth == 0 {
                        obj_start = Some(i);
                    }
                    depth += 1;
                }
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        if let Some(start) = obj_start {
                            let obj = &entries_block[start..=i];
                            entries.push(parse_tv_entry(obj)?);
                            obj_start = None;
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(entries)
    }
}

fn parse_tv_entry(obj: &str) -> Result<Phase1TvEntry, String> {
    let name = extract_string(obj, "name")?;
    let description = extract_string(obj, "description")?;
    let outputs_hex = extract_string(obj, "outputs_hex")?;
    let byte_len_str = extract_number(obj, "byte_len")?;
    let byte_len: usize = byte_len_str
        .parse()
        .map_err(|e| format!("byte_len parse: {e}"))?;
    let verification_command = extract_string(obj, "verification_command")?;

    let inputs_start = obj
        .find("\"inputs\": {")
        .ok_or_else(|| "missing inputs key".to_string())?;
    let after_inputs = inputs_start + "\"inputs\": {".len();
    let inputs_end_rel = obj[after_inputs..]
        .find('}')
        .ok_or_else(|| "missing inputs close".to_string())?;
    let inputs_block = &obj[after_inputs..after_inputs + inputs_end_rel];
    let mut inputs_hex = Vec::new();
    for line in inputs_block.lines() {
        let line = line.trim().trim_end_matches(',');
        if line.is_empty() {
            continue;
        }
        let colon = line
            .find(": \"")
            .ok_or_else(|| "bad input line".to_string())?;
        let key = &line[1..colon];
        let value_start = colon + 3;
        let value_end = line.len() - 1;
        let value = &line[value_start..value_end];
        inputs_hex.push((key.to_string(), value.to_string()));
    }
    Ok(Phase1TvEntry {
        name,
        description,
        inputs_hex,
        outputs_hex,
        byte_len,
        verification_command,
    })
}

fn extract_string(obj: &str, key: &str) -> Result<String, String> {
    let needle = format!("\"{key}\": \"");
    let start = obj
        .find(&needle)
        .ok_or_else(|| format!("missing key: {key}"))?;
    let after = start + needle.len();
    let rest = &obj[after..];
    let end = rest
        .find('"')
        .ok_or_else(|| format!("unterminated key: {key}"))?;
    Ok(rest[..end].to_string())
}

fn extract_number(obj: &str, key: &str) -> Result<String, String> {
    let needle = format!("\"{key}\": ");
    let start = obj
        .find(&needle)
        .ok_or_else(|| format!("missing key: {key}"))?;
    let after = start + needle.len();
    let rest = &obj[after..];
    let end = rest
        .find(|c: char| c == ',' || c == '}' || c.is_whitespace())
        .ok_or_else(|| format!("unterminated number: {key}"))?;
    Ok(rest[..end].to_string())
}

/// Resolve the fixture path from the test's CWD (`crates/octo-wallet/`
/// under `cargo test -p octo-wallet --lib`).
fn fixture_path() -> std::path::PathBuf {
    let mut p = std::env::current_dir().expect("cwd");
    p.push(FIXTURE_PATH);
    p
}

/// Dump mode: regenerate the JSON fixture from the canonical
/// reference impl. Run with `UPDATE_PHASE1_TV=1` to bootstrap (or
/// refresh after a substrate change). The dump is deterministic —
/// identical inputs produce identical outputs across runs.
#[test]
fn phase1_tv_json_dump() {
    if std::env::var("UPDATE_PHASE1_TV").ok().as_deref() != Some("1") {
        eprintln!(
            "phase1_tv_json_dump is a no-op unless UPDATE_PHASE1_TV=1. \
             To bootstrap the fixture, run: \
             UPDATE_PHASE1_TV=1 cargo test -p octo-wallet --lib phase1_tv_json phase1_tv_json_dump -- --nocapture"
        );
        return;
    }
    let fixture = Phase1Fixture::compute();
    let path = fixture_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create fixture dir");
    }
    std::fs::write(&path, fixture.to_json()).expect("write fixture");
    eprintln!("phase1_tv_json fixture written: {}", path.display());
    for e in &fixture.entries {
        eprintln!(
            "  {} ({} bytes): outputs_hex={}",
            e.name, e.byte_len, e.outputs_hex
        );
    }
}

/// TV-1 gate: `MissionId` serde_json round-trip equivalence.
/// Named per RFC-0009 §Test Vectors + §Validation `phase1_tv_json_*` test list.
#[test]
fn phase1_tv_json_v11_round_trip_equivalence() {
    let path = fixture_path();
    let json = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()));
    let entries = Phase1Fixture::from_json(&json)
        .unwrap_or_else(|e| panic!("parse fixture {}: {e}", path.display()));
    let entry = entries
        .iter()
        .find(|e| e.name == "TV-1")
        .expect("TV-1 entry must exist");
    let actual = tv1_v11_round_trip_equivalence();
    let expected = hex::decode(&entry.outputs_hex).expect("outputs_hex must be valid hex");
    assert_eq!(
        actual.len(),
        entry.byte_len,
        "TV-1: byte_len mismatch (fixture says {}, computed {})",
        entry.byte_len,
        actual.len()
    );
    assert_eq!(
        actual.as_slice(),
        expected.as_slice(),
        "TV-1: outputs_hex drift\n  expected: {}\n  actual:   {}\n\
         Re-bootstrap via:\n  UPDATE_PHASE1_TV=1 cargo test -p octo-wallet --lib phase1_tv_json phase1_tv_json_dump -- --nocapture",
        entry.outputs_hex,
        hex::encode(&actual)
    );
}

/// TV-2 gate: child key unlinkability.
#[test]
fn phase1_tv_json_child_unlinkability() {
    let path = fixture_path();
    let json = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()));
    let entries = Phase1Fixture::from_json(&json)
        .unwrap_or_else(|e| panic!("parse fixture {}: {e}", path.display()));
    let entry = entries
        .iter()
        .find(|e| e.name == "TV-2")
        .expect("TV-2 entry must exist");
    let actual = tv2_child_unlinkability();
    let expected = hex::decode(&entry.outputs_hex).expect("outputs_hex must be valid hex");
    assert_eq!(
        actual.len(),
        64,
        "TV-2 must be 64 bytes (2 × 32-byte MissionKey)"
    );
    assert_eq!(
        actual.as_slice(),
        expected.as_slice(),
        "TV-2: outputs_hex drift\n  expected: {}\n  actual:   {}\n\
         Re-bootstrap via:\n  UPDATE_PHASE1_TV=1 cargo test -p octo-wallet --lib phase1_tv_json phase1_tv_json_dump -- --nocapture",
        entry.outputs_hex,
        hex::encode(&actual)
    );
}

/// TV-3 gate: HSM boundary no seed exfil.
#[test]
fn phase1_tv_json_hsm_boundary_no_seed_exfil() {
    let path = fixture_path();
    let json = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()));
    let entries = Phase1Fixture::from_json(&json)
        .unwrap_or_else(|e| panic!("parse fixture {}: {e}", path.display()));
    let entry = entries
        .iter()
        .find(|e| e.name == "TV-3")
        .expect("TV-3 entry must exist");
    let actual = tv3_hsm_boundary_no_seed_exfil();
    let expected = hex::decode(&entry.outputs_hex).expect("outputs_hex must be valid hex");
    assert_eq!(actual.len(), 64, "TV-3 must be 64-byte Ed25519 signature");
    assert_eq!(
        actual.as_slice(),
        expected.as_slice(),
        "TV-3: outputs_hex drift\n  expected: {}\n  actual:   {}\n\
         Re-bootstrap via:\n  UPDATE_PHASE1_TV=1 cargo test -p octo-wallet --lib phase1_tv_json phase1_tv_json_dump -- --nocapture",
        entry.outputs_hex,
        hex::encode(&actual)
    );
}
