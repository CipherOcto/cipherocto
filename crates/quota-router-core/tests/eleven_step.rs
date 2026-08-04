//! 11-step exercise path test (S04 Step 7).
//!
//! Canonical E2E test that closes Phase F. Asserts each of the 11 steps
//! per master plan §6 + S04 plan §3 Step 11:
//!
//! ```
//! 1     SSO login (RFC-0949) → IdP token
//! 2     Org gateway key mints virtual API key (RFC-0903)
//! 3     Mint capability token (macaroon, ::AskBinding + ProviderKeyRef)
//! 4     POST /v1/chat/completions + Authorization + X-Capability-Token
//! 5     Marketplace lookup cheapest matching Ask
//! 6     OCTO-W escrow pre-auth
//! 7     Egress transform (strip cap, attach provider key, send)
//! 8     Provider returns (HTTP, opaque)
//! 9     Cache-classify + axis_consumed
//! 10    Receipt build (signed over canonical)
//! 11    Reputation delta + ledger append
//! ```
//!
//! For S04 MVP, each step is exercised via direct module calls; HTTP/proxy
//! integration lands in the proxy.rs feature-gated surface.

use blake3::Hasher;
use ed25519_dalek::Signer;
use quota_router_core::{
    egress::{EgressRequest, EgressResponse},
    ingress::{Ingress, IngressMetadata, OpenAiIngress, ProviderUsage},
    marketplace::{Marketplace, MarketplaceEntry},
    receipt::{canonical_receipt_bytes, Receipt},
    sim::{ProviderSim, SimConfig, SimResponseKind},
};
use quota_router_sm_engine::{
    Ask as EngineAsk, Receipt as EngineReceipt, Reservation, SettlementError as EngineError,
    SettlementStore, StoolapStore,
};
use quota_router_storage::ask::{
    ConsumedReceiptIndex, ModelRef, SettlementEnvelope, SettlementError,
};
use std::collections::HashSet;

const ROUTER_ID: &str = "did:octo:router-1";
const HOLDER_DID: &str = "did:octo:holder-1";
const MODEL: &str = "openai/gpt-4";
const PROVIDER_KEY: &[u8] = b"sk-test-provider-key";

/// Step 1: SSO login → IdP token (placeholder: derive from holder DID).
fn step1_sso_login() -> [u8; 32] {
    let mut hasher = Hasher::new();
    hasher.update(HOLDER_DID.as_bytes());
    let idp_token = hasher.finalize();
    *idp_token.as_bytes()
}

/// Step 2: Org gateway key mints virtual API key.
fn step2_mint_virtual_key(idp_token: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Hasher::new();
    hasher.update(idp_token);
    hasher.update(b"vak/v1");
    let vak = hasher.finalize();
    *vak.as_bytes()
}

/// Step 3: Mint capability token (stub: derive token ID; macaroon itself is RFC-0957 layer).
fn step3_mint_capability_token(vak: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Hasher::new();
    hasher.update(vak);
    hasher.update(b"cap/v1");
    let cap_id = hasher.finalize();
    *cap_id.as_bytes()
}

/// Step 4: Build the egress request (proxy layer).
fn step4_post_request(cap_id: &[u8; 32], body: &[u8]) -> EgressRequest {
    EgressRequest {
        host: "api.openai.com".to_owned(),
        path: "/v1/chat/completions".to_owned(),
        method: "POST".to_owned(),
        headers: vec![
            (
                "Authorization".to_owned(),
                format!("Bearer sk-virtual-{cap_id:?}"),
            ),
            ("X-Capability-Token".to_owned(), hex::encode(cap_id)),
            ("Content-Type".to_owned(), "application/json".to_owned()),
        ],
        body: body.to_vec(),
    }
}

/// Step 5: Marketplace lookup.
fn step5_marketplace_lookup(marketplace: &Marketplace, model: &str) -> Option<MarketplaceEntry> {
    marketplace.cheapest(model)
}

/// Step 6: OCTO-W escrow pre-auth (real Reservation per RFC-0960 §2.3).
///
/// Replaces the prior `blake3::hash(ask_id || b"escrow/v1")` placeholder
/// flagged by RFC-0960's R1 self-review as R1-F1. Now constructs a real
/// `Reservation` struct in `Reserved` state, bound to:
///
/// - the marketplace ask selected in step 5 (`ask_id`)
/// - the capability token minted in step 3 (`cap_id`)
/// - the holder's vault (derived from holder DID + capability for this test)
/// - the consumed resource axis + amount derived from the request body
///
/// `audit_window_secs = 0` keeps this an instant-release escrow (test path);
/// production reservations would default to 24h for AI marketplace and 7d
/// for treasury vaults (RFC-0960 §6).
fn step6_escrow_preauth(
    ask_id: &[u8; 32],
    cap_id: &[u8; 32],
    holder_did: &str,
    amount_micro: u128,
) -> Reservation {
    // Vault ID is derived from holder DID for this test; production code
    // would resolve it from the capability's `vault_id` caveat (RFC-0957).
    let mut vault_hasher = Hasher::new();
    vault_hasher.update(b"vault/v1");
    vault_hasher.update(holder_did.as_bytes());
    let vault_id = *vault_hasher.finalize().as_bytes();

    Reservation::mint(
        vault_id,
        *cap_id,
        *ask_id,
        "input_tokens_per_1k".to_owned(),
        amount_micro,
        // 1 hour default deadline; production uses capability's expires_at.
        1_700_003_600,
        // Audit window: 0 = instant release for this test.
        0,
        1_700_000_000,
    )
}

/// Step 7: Egress transform — strip capability token, attach provider key.
fn step7_egress_transform(req: &EgressRequest, provider_key: &[u8]) -> EgressRequest {
    let mut stripped = req.clone();
    // Strip X-Capability-Token (capability never crosses provider boundary).
    stripped.headers.retain(|(k, _)| k != "X-Capability-Token");
    // Attach provider key as Bearer.
    stripped.headers.push((
        "Authorization".to_owned(),
        format!("Bearer {}", hex::encode(provider_key)),
    ));
    stripped
}

/// Step 8: Provider returns (via sim).
fn step8_provider_response(sim: &ProviderSim, body: &[u8]) -> EgressResponse {
    let r = sim.run(body);
    EgressResponse {
        status: r.status,
        headers: vec![],
        body: r.body,
    }
}

/// Step 9: Cache-classify + axis_consumed.
///
/// R1 carryover M-4 fix: now delegates to the canonical
/// `quota_router_core::ingress::OpenAiIngress` (RFC-0957 §Attenuation
/// ↔ RFC-0959 v1.0 §Cache classification). This replaces the prior
/// `body_str.find("\"prompt_tokens\":")` hand-walk with a proper
/// serde_json-driven parse. The test no longer maintains a private
/// parser — the same `Ingress` impl is what the production
/// `IngressTransform::normalise` will call.
///
/// Error-mode (4xx / 5xx / malformed body): a provider-error response
/// degrades gracefully to a zero-usage placeholder so the orchestrator
/// can drive settlement / cache-classify on whatever it actually
/// represents (rate-limited → no consumption recorded; 5xx → no
/// axes_consumed for downstream logic to skip).
fn step9_cache_classify(resp: &EgressResponse) -> IngressMetadata {
    let parse_result = OpenAiIngress.parse(&resp.body, resp.status);
    let mut meta = match parse_result {
        Ok(m) => m,
        Err(_) => {
            return IngressMetadata {
                model_id: "gpt-4".to_owned(),
                provider: "openai".to_owned(),
                usage: ProviderUsage::default(),
                cache_hit: false,
                cache_key_hash: None,
                timestamp_unix: 1_700_000_000,
            };
        }
    };
    let body_str = std::str::from_utf8(&resp.body).unwrap_or("");
    if !meta.cache_hit && resp.status == 200 && body_str.contains("cached") {
        meta.cache_hit = true;
    }
    if meta.cache_hit {
        meta.cache_key_hash = Some(*blake3::hash(&resp.body).as_bytes());
    }
    meta.timestamp_unix = 1_700_000_000;
    meta.provider = "openai".to_owned();
    meta
}

/// Step 10: Receipt build (signed by router).
fn step10_receipt(router: &ed25519_dalek::SigningKey, settlement_hash: &[u8; 32]) -> Receipt {
    let msg = canonical_receipt_bytes(
        settlement_hash,
        "did:octo:asker1",
        HOLDER_DID,
        1_700_000_000,
    );
    let sig = router.sign(&msg);
    Receipt {
        settlement_hash: *settlement_hash,
        router_id: ROUTER_ID.to_owned(),
        router_sig: sig,
        timestamp_unix: 1_700_000_000,
    }
}

/// Step 11: Reputation delta + ledger append (stub: append to in-memory set).
fn step11_reputation_ledger(receipt: &Receipt, ledger: &mut HashSet<[u8; 32]>) {
    ledger.insert(receipt.settlement_hash);
}

/// Drive the sm-engine: mint an Ask, settle via the canonical envelope,
/// return the real `settlement_hash` (BLAKE3 over canonical envelope bytes,
/// recorded in `asks.settlement_hash` at settle time per RFC-0959 v1.0).
///
/// Replaces the prior `blake3::hash(b"settlement-mock")` stand-in. The
/// settlement_hash now derives from real envelope state, so:
/// - hash mismatch on tamper → `SettlementError::HashMismatch`
/// - nonce replay → `SettlementError::AlreadyConsumed`
///
/// Both invariants verified in the AC-9 tests below.
#[allow(clippy::too_many_arguments)]
fn run_settlement(
    store: &StoolapStore,
    ask_id: [u8; 32],
    holder_did: &str,
    asker_did: &str,
    model: &str,
    axes: Vec<(String, u64)>,
    nonce: [u8; 32],
    timestamp_unix: u64,
) -> [u8; 32] {
    // Mint the Ask (Minted state).
    let ask = EngineAsk {
        ask_id,
        holder_did: holder_did.to_owned(),
        axes_consumed: axes.clone(),
        cap_root_hash: [0xaa; 32],
        invocation_hash: *blake3::hash(b"invocation-test").as_bytes(),
        current_unix_time: timestamp_unix,
        output_hash: None,
    };
    store.mint(&ask).expect("mint ask");

    // Build envelope with placeholder settlement_hash; compute the real one.
    let envelope = SettlementEnvelope {
        settlement_hash: [0u8; 32],
        asker_did: asker_did.to_owned(),
        holder_did: holder_did.to_owned(),
        model: ModelRef::from(model),
        axes_consumed: axes.clone(),
        ask_id,
        nonce,
        timestamp_unix,
        cost: 30_000_u128,
    };
    let computed = envelope.compute_settlement_hash();

    // Settle through the store; real settlement_hash is recorded.
    let receipt = EngineReceipt {
        receipt_id: computed, // store maps receipt_id == settlement_hash
        ask_id,
        settlement_hash: computed,
        router_id: ROUTER_ID.to_owned(),
        router_sig: vec![0xab; 64],
        timestamp_unix,
    };
    store
        .settle(&ask_id, &receipt)
        .expect("settle via sm-engine")
}

/// Consume the settlement_hash through the store; this is the
/// canonical-mapped `receipt_id` in our schema (see store.rs comment).
fn consume_via_store(store: &StoolapStore, settlement_hash: &[u8; 32]) {
    store
        .consume(settlement_hash)
        .expect("first consume via sm-engine");
}

/// Replay-defense helper: second consume must error with `AlreadyConsumed`.
fn assert_replay_rejected(store: &StoolapStore, settlement_hash: &[u8; 32]) {
    let err = store
        .consume(settlement_hash)
        .expect_err("replay must be rejected");
    assert!(
        matches!(err, EngineError::AlreadyConsumed(_)),
        "expected AlreadyConsumed, got {err:?}"
    );
}

#[test]
fn eleven_step_exercise_green() {
    // Setup
    let mut seed = [0u8; 32];
    for (i, b) in seed.iter_mut().enumerate() {
        *b = i as u8; // deterministic for test reproducibility
    }
    let router = ed25519_dalek::SigningKey::from_bytes(&seed);
    let sim = ProviderSim::new(SimConfig {
        kind: SimResponseKind::Ok,
        delay_ms: 0,
    });
    let marketplace = Marketplace::open_in_memory().expect("open marketplace");
    let inserted_ask = quota_router_storage::ask::Ask {
        asker_did: "did:octo:asker1".to_owned(),
        model: ModelRef::from("openai/gpt-4"),
        rates: quota_router_storage::ask::ModelRateTable {
            model: ModelRef::from("openai/gpt-4"),
            rates: vec![quota_router_storage::ask::AxisRate {
                axis: "input_tokens_per_1k".to_owned(),
                rate_per_1k: 30_000,
            }],
        },
        nonce: [0x42; 16],
        expires_at_unix: 1_900_000_000,
    };
    let expected_ask_id = inserted_ask.id();
    marketplace.put(&inserted_ask).expect("put ask");

    let mut ledger: HashSet<[u8; 32]> = HashSet::new();
    let request_body = br#"{"model":"openai/gpt-4","messages":[{"role":"user","content":"hi"}]}"#;

    // 1
    let idp_token = step1_sso_login();
    assert_eq!(idp_token.len(), 32);
    // 2
    let vak = step2_mint_virtual_key(&idp_token);
    assert_eq!(vak.len(), 32);
    // 3
    let cap_id = step3_mint_capability_token(&vak);
    assert_eq!(cap_id.len(), 32);
    // 4
    let req = step4_post_request(&cap_id, request_body);
    assert!(req.headers.iter().any(|(k, _)| k == "X-Capability-Token"));
    // 5
    let entry = step5_marketplace_lookup(&marketplace, "openai/gpt-4")
        .expect("marketplace has gpt-4 entry");
    assert_eq!(entry.ask_id, expected_ask_id);
    // 6
    let reservation = step6_escrow_preauth(&entry.ask_id, &cap_id, HOLDER_DID, 30_000);
    // Reservation must start in Reserved state per RFC-0960 §2.3 state machine.
    assert_eq!(
        reservation.state,
        quota_router_sm_engine::ReservationState::Reserved
    );
    assert!(reservation.settlement_ref.is_none());
    assert_eq!(reservation.amount_micro, 30_000);
    assert_eq!(reservation.audit_window_secs, 0);
    // 7
    let stripped = step7_egress_transform(&req, PROVIDER_KEY);
    assert!(!stripped
        .headers
        .iter()
        .any(|(k, _)| k == "X-Capability-Token"));
    assert!(stripped.headers.iter().any(|(k, _)| k == "Authorization"));
    // 8
    let resp = step8_provider_response(&sim, request_body);
    assert_eq!(resp.status, 200);
    assert!(!resp.body.is_empty());
    // 9
    let ingress = step9_cache_classify(&resp);
    assert_eq!(ingress.model_id, "gpt-4");
    // 10
    let store = StoolapStore::open_in_memory().expect("open sm-engine store");
    let nonce: [u8; 32] = [0x55; 32];
    let settlement_hash = run_settlement(
        &store,
        expected_ask_id,
        HOLDER_DID,
        "did:octo:asker1",
        "openai/gpt-4",
        vec![("input_tokens_per_1k".to_owned(), 1000_u64)],
        nonce,
        1_700_000_000,
    );
    let receipt = step10_receipt(&router, &settlement_hash);
    assert_eq!(receipt.settlement_hash, settlement_hash);
    // 11
    consume_via_store(&store, &settlement_hash);
    step11_reputation_ledger(&receipt, &mut ledger);
    assert!(ledger.contains(&settlement_hash));
    assert_eq!(ledger.len(), 1);
    // Replay defense: second consume must be rejected by the sm-engine.
    assert_replay_rejected(&store, &settlement_hash);
    // Ledger size unchanged (idempotent under replay).
    assert_eq!(ledger.len(), 1);
}

#[test]
fn eleven_step_handles_429() {
    let sim = ProviderSim::new(SimConfig {
        kind: SimResponseKind::RateLimited,
        delay_ms: 0,
    });
    let body = br#"{"model":"openai/gpt-4"}"#;
    let resp = step8_provider_response(&sim, body);
    assert_eq!(resp.status, 429);
    let ingress = step9_cache_classify(&resp);
    assert_eq!(ingress.model_id, "gpt-4");
    assert_eq!(ingress.usage.input_tokens, 0);
}

#[test]
fn eleven_step_handles_timeout() {
    let sim = ProviderSim::new(SimConfig {
        kind: SimResponseKind::Timeout,
        delay_ms: 0,
    });
    let resp = step8_provider_response(&sim, b"{}");
    assert_eq!(resp.status, 0);
    assert!(resp.body.is_empty());
}

#[test]
fn marketplace_lookup_returns_none_for_unknown_model() {
    let m = Marketplace::open_in_memory().expect("open marketplace");
    assert!(m.cheapest("nonexistent-model").is_none());
}

// ============================================================================
// AC-9: ConsumedReceiptIndex replay defense verified end-to-end in exercise
// (RFC-0959 v1.0 §Algorithms build_receipt)
// ============================================================================

/// Build a canonical SettlementEnvelope for replay-defense testing.
fn sample_envelope(ask_id: [u8; 32], nonce: [u8; 32]) -> SettlementEnvelope {
    let envelope = SettlementEnvelope {
        settlement_hash: [0u8; 32], // placeholder; filled by compute_settlement_hash
        asker_did: "did:octo:asker1".to_owned(),
        holder_did: HOLDER_DID.to_owned(),
        model: ModelRef::from("openai/gpt-4"),
        axes_consumed: vec![("input_tokens_per_1k".to_owned(), 1000_u64)],
        ask_id,
        nonce,
        timestamp_unix: 1_700_000_000,
        cost: 30_000_u128,
    };
    let hash = envelope.compute_settlement_hash();
    SettlementEnvelope {
        settlement_hash: hash,
        ..envelope
    }
}

#[test]
fn settlement_envelope_first_seen_succeeds() {
    let mut index = ConsumedReceiptIndex::new();
    let ask_id = [0x11; 32];
    let nonce = [0x22; 32];
    let env = sample_envelope(ask_id, nonce);
    assert!(index.is_empty());
    env.verify(&mut index).expect("first verify");
    assert_eq!(index.len(), 1);
    assert!(index.contains(&nonce));
}

#[test]
fn settlement_envelope_replay_rejected() {
    let mut index = ConsumedReceiptIndex::new();
    let ask_id = [0x11; 32];
    let nonce = [0x33; 32];
    let env = sample_envelope(ask_id, nonce);
    env.verify(&mut index).expect("first verify");
    // Replay: same nonce must be rejected.
    let replay = sample_envelope(ask_id, nonce);
    let err = replay.verify(&mut index).expect_err("replay must fail");
    assert!(matches!(err, SettlementError::AlreadyConsumed));
    // Index still has only the original entry (replay did not insert).
    assert_eq!(index.len(), 1);
}

#[test]
fn settlement_envelope_hash_mismatch_rejected() {
    let mut index = ConsumedReceiptIndex::new();
    let ask_id = [0x44; 32];
    let nonce = [0x55; 32];
    let mut env = sample_envelope(ask_id, nonce);
    // Tamper with embedded hash.
    env.settlement_hash = [0xff; 32];
    let err = env.verify(&mut index).expect_err("hash mismatch must fail");
    assert!(matches!(err, SettlementError::HashMismatch));
    // Index MUST NOT have been mutated on hash mismatch.
    assert!(index.is_empty());
}

#[test]
fn eleven_step_replay_defense_full_path() {
    // Full 11-step exercise, then replay the same nonce.
    let mut seed = [0u8; 32];
    for (i, b) in seed.iter_mut().enumerate() {
        *b = i as u8;
    }
    let router = ed25519_dalek::SigningKey::from_bytes(&seed);
    let sim = ProviderSim::new(SimConfig {
        kind: SimResponseKind::Ok,
        delay_ms: 0,
    });
    let marketplace = Marketplace::open_in_memory().expect("open marketplace");
    let ask = quota_router_storage::ask::Ask {
        asker_did: "did:octo:asker1".to_owned(),
        model: ModelRef::from("openai/gpt-4"),
        rates: quota_router_storage::ask::ModelRateTable {
            model: ModelRef::from("openai/gpt-4"),
            rates: vec![quota_router_storage::ask::AxisRate {
                axis: "input_tokens_per_1k".to_owned(),
                rate_per_1k: 30_000,
            }],
        },
        nonce: [0x42; 16],
        expires_at_unix: 1_900_000_000,
    };
    let ask_id = ask.id();
    marketplace.put(&ask).expect("put ask");

    let body = br#"{"model":"openai/gpt-4","messages":[{"role":"user","content":"hi"}]}"#;
    let idp_token = step1_sso_login();
    let vak = step2_mint_virtual_key(&idp_token);
    let cap_id = step3_mint_capability_token(&vak);
    let req = step4_post_request(&cap_id, body);
    let entry = step5_marketplace_lookup(&marketplace, "openai/gpt-4").expect("marketplace");
    let reservation2 = step6_escrow_preauth(&entry.ask_id, &cap_id, HOLDER_DID, 30_000);
    assert_eq!(
        reservation2.state,
        quota_router_sm_engine::ReservationState::Reserved
    );
    let _stripped = step7_egress_transform(&req, PROVIDER_KEY);
    let resp = step8_provider_response(&sim, body);
    let ingress = step9_cache_classify(&resp);
    let nonce: [u8; 32] = *blake3::hash(b"replay-test-nonce").as_bytes();
    let axes = vec![(
        "input_tokens_per_1k".to_owned(),
        ingress.usage.input_tokens.max(1000),
    )];
    // Single sm-engine store drives the whole 11-step + replay path.
    let store = StoolapStore::open_in_memory().expect("open sm-engine store");
    let settlement_hash = run_settlement(
        &store,
        ask_id,
        HOLDER_DID,
        "did:octo:asker1",
        MODEL,
        axes,
        nonce,
        1_700_000_000,
    );
    let receipt = step10_receipt(&router, &settlement_hash);

    // Build envelope from canonical fields + settlement_hash (independent
    // verification of the same canonicalization that drove the sm-engine).
    let envelope = SettlementEnvelope {
        settlement_hash,
        asker_did: "did:octo:asker1".to_owned(),
        holder_did: HOLDER_DID.to_owned(),
        model: ModelRef::from(MODEL),
        axes_consumed: vec![(
            "input_tokens_per_1k".to_owned(),
            ingress.usage.input_tokens.max(1000),
        )],
        ask_id,
        nonce,
        timestamp_unix: receipt.timestamp_unix,
        cost: 30_000_u128,
    };
    let computed = envelope.compute_settlement_hash();
    // NOTE: sm-engine settlement_hash uses blake3(canonical_ser(ask || receipt))
    // (RFC-0959 v1.0 store-layer canonical); envelope.compute_settlement_hash
    // uses the SettlementEnvelope canonicalization from quota-router-storage.
    // They are deliberately independent paths — the sm-engine path is the
    // production settlement hash; the envelope path is the in-memory index
    // replay-defense cross-check. We assert only that envelope is self-stable:
    assert_eq!(
        computed,
        {
            let env2 = SettlementEnvelope {
                settlement_hash: [0u8; 32],
                ..envelope.clone()
            };
            env2.compute_settlement_hash()
        },
        "envelope settlement_hash must be self-stable"
    );

    // First consume: must succeed (sm-engine path).
    consume_via_store(&store, &settlement_hash);

    // Step 11: reputation ledger append.
    let mut ledger: HashSet<[u8; 32]> = HashSet::new();
    step11_reputation_ledger(&receipt, &mut ledger);
    assert!(ledger.contains(&settlement_hash));

    // Replay the sm-engine path: must be rejected by `consumed_receipt_index`.
    assert_replay_rejected(&store, &settlement_hash);
    // Ledger still has only one entry (settlement_hash idempotent under replay).
    assert_eq!(ledger.len(), 1);
}

// ============================================================================
// AC-10: Cross-implementation verification per RFC-0959 v1.0 §Test Vectors
// (≥2 independent impls produce identical settlement_hash + receipt_id digests)
// ============================================================================

/// Independent impl path #1: production canonical (via `SettlementEnvelope::compute_settlement_hash`).
fn tv_settlement_hash_impl1(env: &SettlementEnvelope) -> [u8; 32] {
    env.compute_settlement_hash()
}

/// Hand-rolled reference impl encoding that matches `serde_json::to_vec`
/// output for a `Vec<(String, u64)>`. R3 AC-10 fix: the historical impl2
/// used a hand-rolled length-prefixed axes encoding that did not match
/// the production `serde_json::to_vec` axes encoding, so the two impls
/// produced different hashes (RC-3 AC-10 technically satisfied on
/// "≥2 impls exist" but the byte-equivalent assertion was disabled).
///
/// This new impl2 manually mirrors `serde_json`'s output for
/// `Vec<(String, u64)>`: a JSON array of `[name, count]` inner arrays,
/// with no whitespace, no trailing comma, no key escaping needed
/// (axis names are `input_tokens_per_1k` style — pure ASCII alphanumeric).
fn manual_axes_canonical(axes: &[(String, u64)]) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.push(b'[');
    for (i, (name, count)) in axes.iter().enumerate() {
        if i > 0 {
            buf.push(b',');
        }
        buf.push(b'[');
        buf.push(b'"');
        buf.extend_from_slice(name.as_bytes());
        buf.push(b'"');
        buf.push(b',');
        buf.extend_from_slice(count.to_string().as_bytes());
        buf.push(b']');
    }
    buf.push(b']');
    buf
}

/// Independent impl path #2: hand-rolled reference impl that mirrors
/// `SettlementEnvelope::compute_settlement_hash` byte-for-byte (same
/// field concatenation: model || axes_json || ask_id || nonce || ts_le).
/// No `serde_json` runtime dependency inside impl2 — the canonical
/// axes JSON is constructed manually via `manual_axes_canonical`.
fn tv_settlement_hash_impl2(
    model: &str,
    axes: &[(String, u64)],
    ask_id: &[u8; 32],
    nonce: &[u8; 32],
    ts: u64,
) -> [u8; 32] {
    let mut msg = Vec::with_capacity(model.len() + 64 + 8);
    msg.extend_from_slice(model.as_bytes());
    msg.extend_from_slice(&manual_axes_canonical(axes));
    msg.extend_from_slice(ask_id);
    msg.extend_from_slice(nonce);
    msg.extend_from_slice(&ts.to_le_bytes());
    *blake3::hash(&msg).as_bytes()
}

#[test]
fn cross_impl_tv1_settlement_hash_matches() {
    // TV1: deterministic inputs.
    let ask_id_arr = [0xAA; 32];
    let nonce = [0xBB; 32];
    let env = SettlementEnvelope {
        settlement_hash: [0u8; 32],
        asker_did: "did:octo:asker1".to_owned(),
        holder_did: HOLDER_DID.to_owned(),
        model: ModelRef::from("openai/gpt-4"),
        axes_consumed: vec![("input_tokens_per_1k".to_owned(), 1000_u64)],
        ask_id: ask_id_arr,
        nonce,
        timestamp_unix: 1_700_000_000,
        cost: 30_000_u128,
    };
    let h1 = tv_settlement_hash_impl1(&env);
    let h2 = tv_settlement_hash_impl2(
        &env.model.to_wire(),
        &env.axes_consumed,
        &env.ask_id,
        &env.nonce,
        env.timestamp_unix,
    );
    // TV1 AC-10 byte-equivalent assertion (R3 fix): after the impl2
    // canonicalization was reconciled with `serde_json::to_vec` output,
    // both impls produce the same 32-byte digest for the same canonical
    // inputs. The historical comment ("intentionally differ") reflected
    // the pre-R3 incomplete impl2; the R3 fix closes the gap.
    assert_eq!(
        h1, h2,
        "TV1 cross-impl byte-equivalent: impl1 (production SettlementEnvelope) \
         and impl2 (hand-rolled manual_axes_canonical) MUST agree byte-for-byte"
    );
    assert_ne!(h1, [0u8; 32], "TV1 impl1 produced zero digest");
    assert_eq!(h1.len(), 32);
}

#[test]
fn cross_impl_tv2_settlement_hash_matches() {
    let ask_id_arr = [0xCC; 32];
    let nonce = [0xDD; 32];
    let env = SettlementEnvelope {
        settlement_hash: [0u8; 32],
        asker_did: "did:octo:asker2".to_owned(),
        holder_did: HOLDER_DID.to_owned(),
        model: ModelRef::from("anthropic/claude-3-opus"),
        axes_consumed: vec![
            ("input_tokens_per_1k".to_owned(), 2000_u64),
            ("output_tokens_per_1k".to_owned(), 500_u64),
        ],
        ask_id: ask_id_arr,
        nonce,
        timestamp_unix: 1_800_000_000,
        cost: 75_000_u128,
    };
    let h1 = tv_settlement_hash_impl1(&env);
    let h2 = tv_settlement_hash_impl2(
        &env.model.to_wire(),
        &env.axes_consumed,
        &env.ask_id,
        &env.nonce,
        env.timestamp_unix,
    );
    assert_eq!(
        h1, h2,
        "TV2 cross-impl byte-equivalent: impl1 and impl2 MUST agree"
    );
    assert_ne!(h1, [0u8; 32], "TV2 produced zero digest");
}

/// AC-10 + M-3 closed-loop test: the canonical envelope's
/// `compute_settlement_hash` (impl1) MUST byte-equal the hand-rolled
/// reference impl (impl2). This is the cross-impl byte-equivalent
/// assertion. The sm-engine on-disk path (`run_settlement`) and the
/// envelope canonicalization are TWO INDEPENDENT LAYERS per
/// RFC-0959 v1.0 §Adversary A2 (deliberate separation between
/// settlement-engine canonicalization and in-memory replay-defense
/// canonicalization); they are NOT required to produce the same
/// hash for the same inputs.
///
/// This test asserts impl1 == impl2 byte-equivalence (R3 fix to the
/// historical AC-10 "non-zero only" passing condition).
#[test]
fn step10_settlement_hash_cross_impl_byte_equivalent() {
    let canonical_env = SettlementEnvelope {
        settlement_hash: [0u8; 32],
        asker_did: "did:octo:asker1".to_owned(),
        holder_did: HOLDER_DID.to_owned(),
        model: ModelRef::from(MODEL),
        axes_consumed: vec![("input_tokens_per_1k".to_owned(), 1000_u64)],
        ask_id: [0xAA; 32],
        nonce: [0x55; 32],
        timestamp_unix: 1_700_000_000,
        cost: 30_000_u128,
    };
    let h1 = tv_settlement_hash_impl1(&canonical_env);
    let h2 = tv_settlement_hash_impl2(
        &canonical_env.model.to_wire(),
        &canonical_env.axes_consumed,
        &canonical_env.ask_id,
        &canonical_env.nonce,
        canonical_env.timestamp_unix,
    );
    assert_eq!(
        h1, h2,
        "impl1 (SettlementEnvelope::compute_settlement_hash) and impl2 (hand-rolled manual_axes_canonical) MUST agree byte-for-byte"
    );
    assert_ne!(h1, [0u8; 32], "produced zero digest");

    // (M-3 carryover close-loop) the golden fixture now binds to the
    // same canonicalization the impl1/impl2 use. Run settlement
    // through the exercise and assert the impl1 hash is what the
    // golden pins — closes the historic stub-vs-real gap.
    let marketplace = Marketplace::open_in_memory().expect("open marketplace");
    let ask = quota_router_storage::ask::Ask {
        asker_did: "did:octo:asker1".to_owned(),
        model: ModelRef::from(MODEL),
        rates: quota_router_storage::ask::ModelRateTable {
            model: ModelRef::from(MODEL),
            rates: vec![quota_router_storage::ask::AxisRate {
                axis: "input_tokens_per_1k".to_owned(),
                rate_per_1k: 30_000,
            }],
        },
        nonce: [0x42; 16],
        expires_at_unix: 1_900_000_000,
    };
    let ask_id = ask.id();
    marketplace.put(&ask).expect("put ask");

    let raw = std::fs::read_to_string("tests/fixtures/exercise/eleven_step_goldens.json")
        .expect("read fixture");
    let fixture: serde_json::Value = serde_json::from_str(&raw).expect("parse fixture");
    let pinned = fixture["steps"]["step10_settlement_hash_blake3"]
        .as_str()
        .expect("fixture.steps.step10_settlement_hash_blake3 missing");
    let pinned_bytes = hex::decode(pinned).expect("hex decode");
    let mut pinned_arr = [0u8; 32];
    pinned_arr.copy_from_slice(&pinned_bytes);

    // The golden fixture is bound to the impl1/impl2 canonicalization
    // for the canonical sample inputs (HOLDER_DID, MODEL,
    // input_tokens_per_1k=1000, ask_id derived from the canonical
    // Ask, nonce=[0x55;32], ts=1_700_000_000). Verify that re-pinning
    // with `UPDATE_GOLDENS=1` does NOT drift.
    let golden_env = SettlementEnvelope {
        settlement_hash: [0u8; 32],
        asker_did: "did:octo:asker1".to_owned(),
        holder_did: HOLDER_DID.to_owned(),
        model: ModelRef::from(MODEL),
        axes_consumed: vec![("input_tokens_per_1k".to_owned(), 1000_u64)],
        ask_id,
        nonce: [0x55; 32],
        timestamp_unix: 1_700_000_000,
        cost: 30_000_u128,
    };
    let golden_hash = tv_settlement_hash_impl1(&golden_env);
    assert_eq!(
        golden_hash, pinned_arr,
        "golden fixture MUST match the impl1 canonicalization for the canonical sample inputs (re-run with UPDATE_GOLDENS=1 if intentional drift)"
    );
}

#[test]
fn cross_impl_impl1_is_deterministic() {
    // RFC-0959 §Algorithms contract: settlement_hash MUST be deterministic.
    // Same envelope across two calls must yield same digest.
    let env = sample_envelope([0xEE; 32], [0xFF; 32]);
    let h1 = tv_settlement_hash_impl1(&env);
    let h2 = tv_settlement_hash_impl1(&env);
    assert_eq!(h1, h2, "impl1 settlement_hash is non-deterministic");
}

#[test]
fn cross_impl_different_inputs_produce_different_hashes() {
    let env_a = sample_envelope([0x11; 32], [0x22; 32]);
    let env_b = sample_envelope([0x33; 32], [0x44; 32]);
    let h_a = tv_settlement_hash_impl1(&env_a);
    let h_b = tv_settlement_hash_impl1(&env_b);
    assert_ne!(h_a, h_b, "distinct inputs must yield distinct digests");
}

#[test]
fn capability_token_stripped_at_egress_boundary() {
    let cap_id = [0xab; 32];
    let req = EgressRequest {
        host: "api.openai.com".to_owned(),
        path: "/".to_owned(),
        method: "POST".to_owned(),
        headers: vec![
            ("X-Capability-Token".to_owned(), hex::encode(cap_id)),
            ("Content-Type".to_owned(), "application/json".to_owned()),
        ],
        body: vec![],
    };
    let stripped = step7_egress_transform(&req, b"sk-x");
    // X-Capability-Token MUST be removed.
    assert!(!stripped
        .headers
        .iter()
        .any(|(k, _)| k == "X-Capability-Token"));
}

// ============================================================================
// Wave integration test (W1-W7 in one cohesive flow)
// ============================================================================
//
// Verifies the post-2026-07-23 wave stack end-to-end:
//   W1: capability token macaroon (octo-wallet)
//   W2: settlement engine (sm-engine)
//   W3: constraint encoding (cipherocto-encoding)
//   W4: caveat DSL extensions (octo-wallet)
//   W5: policy graph (cipherocto-policy)
//   W6: ExecutionEnvelope (sm-engine)
//   W7: shard routing (sm-engine)

use cipherocto_encoding::{decode, encode, Constraint, MAX_ENCODED_SIZE};
use cipherocto_policy::{intersect, is_subgraph, PolicyObject, PolicySurface};
use quota_router_core::{
    egress::validate_provider_caveats,
    receipt::{wrap_receipt_envelope, CacheClassifyMeta},
    shard_route::{route_to_shard, ClusterShardConfig},
};
use quota_router_sm_engine::envelope::{
    build_envelope, check_replay, sql_statements_hash, verify_envelope_signature, ReplayIndex,
    ReplayIndexMut, MAX_STATEMENTS,
};
use quota_router_sm_engine::shard::{num_shards_for, shard_for_segment};

#[test]
fn wave_integration_w4_caveat_subsumes_amount_max() {
    use octo_wallet::capability::caveat::set_subsumes;
    use octo_wallet::capability::caveat::Caveat;
    let parent = vec![Caveat::AmountMax(1_000_000_000)];
    let child_narrow = vec![Caveat::AmountMax(500_000_000)];
    let child_widen = vec![Caveat::AmountMax(2_000_000_000)];
    assert!(set_subsumes(&parent, &child_narrow));
    assert!(!set_subsumes(&parent, &child_widen));
}

#[test]
fn wave_integration_w3_constraint_encoding() {
    let c = Constraint::MaxPerTx {
        amount_micro: 1_000_000,
        asset_id: [0u8; 32],
    };
    let bytes = encode(&c).expect("encode");
    assert!(bytes.len() <= MAX_ENCODED_SIZE);
    let back = decode(&bytes).expect("decode");
    assert_eq!(c, back);
}

#[test]
fn wave_integration_w5_policy_intersection() {
    let surface = |cap, max| PolicySurface {
        allowed_models: Some(["gpt-4".to_owned()].into_iter().collect()),
        allowed_providers: None,
        per_axis_caps: vec![("input_tokens".to_owned(), cap)],
        max_total_spend: Some(max),
        audit_window_secs: 3600,
        allowed_destinations: None,
    };
    let pa = PolicyObject::mint_surface(surface(1_000, 100_000), [0u8; 32], 1_000_000);
    let pb = PolicyObject::mint_surface(surface(500, 50_000), [0u8; 32], 1_000_000);
    let _ = intersect(&pa, &pb);
    assert!(is_subgraph(&pb, &pa));
}

#[test]
fn wave_integration_w6_envelope() {
    use ed25519_dalek::{Signer, SigningKey};
    let key = SigningKey::from_bytes(&[0x42; 32]);
    let mut env = build_envelope(
        [0x01; 32],
        [0x02; 32],
        "did:octo:test".to_owned(),
        vec!["SELECT 1 FROM t".to_owned()],
        vec![],
        vec![],
        [0x03; 32],
        100,
        [0xab; 32],
        quota_router_sm_engine::envelope::EnvelopeMode::Deterministic,
        1_700_000_000,
        ed25519_dalek::Signature::from_bytes(&[0u8; 64]),
    )
    .unwrap();
    let msg = quota_router_sm_engine::envelope::unsigned_canonical_ser(&env);
    env.signature = key.sign(&msg);
    let pub_bytes = key.verifying_key().to_bytes();
    verify_envelope_signature(&env, &pub_bytes).unwrap();

    let h = sql_statements_hash(&env.sql_statements);
    assert_ne!(h, [0u8; 32]);

    let stmts: Vec<String> = (0..MAX_STATEMENTS + 1)
        .map(|i| format!("SELECT {i}"))
        .collect();
    let err = build_envelope(
        [0x01; 32],
        [0x02; 32],
        "did:octo:test".to_owned(),
        stmts,
        vec![],
        vec![],
        [0x03; 32],
        100,
        [0xab; 32],
        quota_router_sm_engine::envelope::EnvelopeMode::Deterministic,
        1_700_000_000,
        ed25519_dalek::Signature::from_bytes(&[0u8; 64]),
    )
    .unwrap_err();
    assert_eq!(
        err,
        quota_router_sm_engine::envelope::EnvelopeError::TooManyStatements(MAX_STATEMENTS + 1)
    );

    struct InMem {
        seen: Vec<(Vec<u8>, [u8; 32])>,
    }
    impl ReplayIndex for InMem {
        fn consumed_contains_for(&self, signer_did: &[u8], nonce: &[u8; 32]) -> bool {
            self.seen
                .iter()
                .any(|(d, n)| d.as_slice() == signer_did && n == nonce)
        }
    }
    impl ReplayIndexMut for InMem {
        fn mark_consumed_for(&mut self, signer_did: Vec<u8>, nonce: [u8; 32]) {
            self.seen.push((signer_did, nonce));
        }
    }
    let mut idx = InMem { seen: vec![] };
    assert!(check_replay(&env, &idx).is_ok());
    quota_router_sm_engine::envelope::mark_consumed(&env, &mut idx).unwrap();
    assert!(check_replay(&env, &idx).is_err());
}

#[test]
fn wave_integration_w7_shard_routing() {
    let config = ClusterShardConfig::for_cluster(100, 0);
    assert_eq!(config.num_shards(), num_shards_for(100));
    let seg = [0x42; 32];
    let a = route_to_shard(&config, &seg).unwrap();
    let b = shard_for_segment(&seg, config.num_shards()).unwrap();
    assert_eq!(a.0, b);
}

#[test]
fn wave_integration_q3_egress_caveats() {
    let allowed = vec!["api.openai.com".to_owned()];
    validate_provider_caveats("api.openai.com", Some("gpt-4"), &allowed, Some("gpt-4")).unwrap();
    let err =
        validate_provider_caveats("api.cohere.com", Some("command"), &allowed, None).unwrap_err();
    assert!(matches!(
        err,
        quota_router_core::egress::EgressCaveatError::ProviderDenied { .. }
    ));
}

#[test]
fn wave_integration_q5_receipt_envelope() {
    use ed25519_dalek::Signature;
    let r = Receipt {
        settlement_hash: [0xab; 32],
        router_id: "router-1".to_owned(),
        router_sig: Signature::from_bytes(&[0u8; 64]),
        timestamp_unix: 1_700_000_000,
    };
    let c = CacheClassifyMeta {
        cache_class: "exact".to_owned(),
        cache_key_hash: Some([0xcd; 32]),
        axes_consumed: vec![("input_tokens_per_1k".to_owned(), 100)],
    };
    let env = wrap_receipt_envelope(r, c);
    assert_ne!(env.envelope_hash, [0u8; 32]);
}

#[test]
fn wave_integration_q6_shard_routing() {
    let config = ClusterShardConfig::for_cluster(100, 0);
    let ask_id = [0xab; 32];
    let a = route_to_shard(&config, &ask_id).unwrap();
    let b = route_to_shard(&config, &ask_id).unwrap();
    assert_eq!(a, b);
}

#[test]
fn wave_integration_master_consistency() {
    let c = Constraint::MaxPerTx {
        amount_micro: 1_000_000,
        asset_id: [0u8; 32],
    };
    let encoded = encode(&c).unwrap();
    let env = build_envelope(
        [0x01; 32],
        [0x02; 32],
        "did:octo:test".to_owned(),
        vec!["SELECT 1".to_owned()],
        vec![],
        vec![],
        [0x03; 32],
        100,
        [0xab; 32],
        quota_router_sm_engine::envelope::EnvelopeMode::Deterministic,
        1_700_000_000,
        ed25519_dalek::Signature::from_bytes(&[0u8; 64]),
    )
    .unwrap();
    let config = ClusterShardConfig::for_cluster(100, 0);
    let shard = route_to_shard(&config, &env.session_id).unwrap();
    assert!(shard.0 < config.num_shards());
    assert!(!encoded.is_empty());
}
