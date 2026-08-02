# RFC-0969 (Economics): Dual-Pipeline Authorization — Legacy Bearer + Capability

## Status

Accepted (promoted 2026-08-02)

## Authors

- Author: @mmacedoeu
- Contributor: @mmacedoeu

## Maintainers

- Maintainer: @mmacedoeu
- Maintainer: @mmacedoeu

## Summary

Closes G1 ("no spec for dual-pipeline auth") + G2 ("dual-issuance is not a spec concept") by canonically specifying that the cipherocto gateway accepts both legacy bearer tokens (RFC-0903) and capability tokens (RFC-0957) on the same request envelope, routes them through distinct verification paths that share infrastructure, and produces a unified `HolderRecord` in the destination node's `HolderRegistry` (RFC-0957-A1). This RFC does NOT change the wire format of either token; it specifies the routing.

Key elements:

1. **Wire Format** — `Authorization: Bearer <sk-...>` OR `X-Capability-Token: <macaroon>` OR `Authorization: CipherOcto-Cap <macaroon>` (the alt path mentioned in RFC-0957). Each header alone, or `Bearer` + `X-Capability-Token` together, is valid.
2. **Gateway Authenticator** — new role on the gateway. Owns the header parser + router + dispatch. Stateless beyond the local `HolderRegistry` cache.
3. **Routing algorithm** — header prefix determines parse path:
   - `Authorization: Bearer <...>` → RFC-0903 path (virtual-key table lookup + vault borrow)
   - `X-Capability-Token: <...>` → RFC-0957 path (HolderRegistry lookup + macaroon chain check + Ed25519 sig verify)
   - `Authorization: CipherOcto-Cap <...>` → RFC-0957 alt path
4. **Dual-issuance** — a holder can hold both a bearer (from RFC-0903) and a capability token (from RFC-0957) for the same `HolderRecord`. The destination node's mint endpoint accepts either request and writes to the same `HolderRegistry`.
5. **Identity linkage** — when both `Bearer` and `X-Capability-Token` are present, the gateway requires the bearer's `subject_did` (or a virtual-key→holder binding) to equal the capability's `HolderRecord.holder_did`, AND the bearer's `ask_id` to equal the capability's `ask_id`. Mismatch is rejected with `IdentityMismatch`. This is the Round 2 fix for the cross-holder credential mixing attack.
6. **`mint_dual` algorithm** — `mint_dual(buyer_did, buyer_holder_pub, ask_id, ask_ttl_unix, capability_root_secret, buyer_encryption_pubkey, wallet, db)` — 8 params (R22-N1 fix: dropped dead `bearer_root_secret`; canonical signature per §`mint_dual` Algorithm at line 475 uses 8 params with `&dyn WalletCrypto` + `&stoolap::Database`). Mints both tokens exactly once and writes both via `TransactionExt::insert_dual` (RFC-0957-A1 §TransactionExt). `CapabilityToken::mint(root_secret, holder, holder_did, initial_caveats)` is the 4-arg persistence-free signature (R6-C3 fix, R7-N6 fix: NO `Some(&mut txn)` parameter; mint is pure crypto, no post-write hook). The caller writes both `HolderRecord`s via `txn.insert_dual(...)` in the same transaction, preventing the double-insert contradiction. (R15-N16 fix: `mint_dual` is a preview/test utility; production writes happen ONLY in `deliver_at_settlement` per RFC-0959-A1 §Algorithms. Calling both paths for the same ask_id would hit `UNIQUE(ask_id, kind)` and fail. Documented as the single-write authority.)
7. **Backward compat** — legacy clients (claude-code, hardcoded HTTP agents) using `Authorization: Bearer <sk-...>` continue working without client-side changes.
8. **Forward compat** — new clients opt into capability by including the wallet-side signer and `X-Capability-Token` header.
9. **Debug redaction** — `AuthHeader`, `DispatchSet`, `AuthenticatedIdentity`, `BearerVerification`, `CapabilityVerification` use manual `impl Debug` with redaction.

## Why Needed

The dual-mode workflow requires two authorization pipelines running through the same gateway:

1. **Legacy bearer** — `Authorization: Bearer <sk-...>` for legacy clients. Validated by the gateway as a virtual-API-key (RFC-0903) or enterprise SSO token (RFC-0949, out of scope).
2. **Capability-based** — `X-Capability-Token: <3-segment macaroon>` for new wallet-side clients. Verified by RFC-0957 macaroon chain + Ed25519 holder signature + discharge channels.

Today's RFCs scatter this story. No document names the Gateway Authenticator role. No document specifies header-prefix routing. No document states that both tokens can coexist on the same request envelope with identity linkage.

This RFC names the role, specifies the routing, binds identity linkage, and binds both paths to the unified catalog.

## Scope

### In Scope

- Wire format coexistence (header names + parse paths).
- Gateway Authenticator role definition.
- Header-prefix routing algorithm.
- Dual-issuance semantics (one holder, two tokens, one registry entry).
- Header precedence rules (AND-gate when both present + identity linkage).
- Backward compat with RFC-0903 path.
- Forward compat with RFC-0957 path + RFC-0957-A1 catalog.
- Test vectors for routing, dual-issuance, header precedence, identity mismatch, error cases.

### Out of Scope

- **Wire format of either token** — RFC-0903 + RFC-0957 authoritative.
- **Mint API for either token** — RFC-0903 + RFC-0957 + RFC-0957-A1 authoritative.
- **Catalog storage** — RFC-0957-A1 authoritative.
- **Market delivery** — RFC-0959-A1 authoritative.
- **Forwarding-hop auth** — RFC-0970 authoritative.
- **Role consolidation** — RFC-0971 authoritative.
- **Enterprise SSO** — RFC-0949 authoritative.
- **Dual-mode provider integration** — RFC-0917 authoritative; orthogonal.

## Dependencies

**Requires:**

- RFC-0903 — virtual keys + bearer emission + verification path
- RFC-0957 — capability wire + macaroon chain
- RFC-0957-A1 — unified HolderRegistry + Transaction type + CapabilityCatalog extensions
- RFC-0009 — Identity substrate

**Optional:**

- RFC-0949 — Enterprise SSO forward-compat hook
- RFC-0959-A1 — Market Delivery (delivery populates the registry on both paths)

**Not Requires:**

- RFC-0909 — coexistence only

> **Dependency Validation Rules:**
> 1. DAG: `0969 ← {0903, 0957, 0957-A1, 0009, 0009-B1, 0949*, 0959-A1*}` — acyclic (R11-N9 fix: added `0009-B1` per §Dependencies RFC-0009-B1 entry)
> 2. RFC-0957-A1 HolderRegistry substrate prerequisite
> 3. RFC-0903 + RFC-0957 + RFC-0009 prerequisites satisfied
> 4. RFC-0959-A1 is OPTIONAL integration, not blocking

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| **G1: Routing determinism** | Header prefix → parse path is a pure function | Test: every header prefix has exactly one parse path |
| **G2: Dual-pipeline coverage** | 100% of gateway requests routed through bearer path, capability path, or both (AND-gate) | Test: 100 representative requests, all routed correctly |
| **G3: Header precedence** | When both Bearer + Capability-Token are present, both paths must verify AND identity must match | Test: dual-token request with linked identity → Ok; with mismatched identity → Err |
| **G4: Backward compat** | Legacy bearer requests continue working | Regression test over RFC-0903 |
| **G5: Forward compat** | Wallet-side capability requests work | Regression test over RFC-0957 |
| **G6: Routing latency** | ≤ 1ms p99 added | Bench |
| **G7: Catalog unification** | Both paths write to the same HolderRegistry via insert_dual | Test: dual-issuance populates two paired HolderRecord rows in one transaction |
| **G8: Debug redaction** | Zero credential material in Debug | Test: TV10 |

## Motivation

### Problem Statement

Today, a request arrives at the gateway with `Authorization: Bearer <sk-...>`. The gateway validates against the RFC-0903 virtual-key table. Tomorrow, the same gateway must accept `X-Capability-Token: <macaroon>` requests and route both with identity linkage.

A coherent gateway MUST handle both. This RFC specifies the how.

### Desired State

A request arrives at the gateway:

```
GET /v1/inference HTTP/1.1
Host: gateway.cipherocto
Authorization: Bearer sk-cipherocto-abc123
X-Capability-Token: eyJ...macaroon....eyJ...sig....eyJ...discharges
X-Request-Id: req_42
```

The gateway's Gateway Authenticator:
1. Parses `Authorization: Bearer sk-...` → routes to RFC-0903 path.
2. Parses `X-Capability-Token: <...>` → routes to RFC-0957 path.
3. Both paths MUST succeed before forwarding (header precedence rule: AND).
4. The RFC-0957 path looks up the `HolderRegistry` for the `cap_root_hash`; succeeds → capability verified.
5. The RFC-0903 path looks up the virtual-key table; succeeds → bearer verified.
6. **Identity linkage check**: the bearer's `subject_did` (or virtual-key→holder binding) MUST equal the capability's `HolderRecord.holder_did`; the bearer's `ask_id` MUST equal the capability's `ask_id`. If they differ → `Err(AuthError::IdentityMismatch)`.
7. Both succeed AND identity matches → request is forwarded.

If either fails, the request is rejected with a specific error code.

### Use Case Link

`docs/use-cases/dual-mode-authorization-workflow.md`

## Specification

### System Architecture

```mermaid
graph TB
    REQ[Inbound Request] --> GA[Gateway Authenticator]
    GA --> PARSE[Header Parser]
    PARSE --> B[Authorization: Bearer]
    PARSE --> C1[X-Capability-Token]
    PARSE --> C2[Authorization: CipherOcto-Cap]
    B --> P1[RFC-0903 path<br/>virtual-key lookup]
    C1 --> P2[RFC-0957 path<br/>HolderRegistry lookup + chain check]
    C2 --> P2
    P1 --> LINK[Identity Linkage Check<br/>bearer.subject_did == cap.holder_did<br/>bearer.ask_id == cap.ask_id]
    P2 --> LINK
    LINK -->|all pass| FWD[Forward to upstream]
    LINK -->|any fail| ERR[401 with error code]
    P1 --> REG[HolderRegistry<br/>RFC-0957-A1]
    P2 --> REG
    P1 --> VK[Virtual-Key Table<br/>RFC-0903]
```

### Data Structures

#### `AuthHeader` enum

```rust
/// Per RFC-0969 §Data Structures.
pub enum AuthHeader {
    /// RFC-0903 bearer: `Authorization: Bearer <sk-...>`.
    Bearer { token: String },

    /// RFC-0957 capability token: `X-Capability-Token: <macaroon>`.
    CapabilityToken { token: String },

    /// RFC-0957 alt path: `Authorization: CipherOcto-Cap <macaroon>`.
    CipherOctoCap { token: String },
}

// Manual Debug redaction.
impl std::fmt::Debug for AuthHeader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthHeader::Bearer { .. } => f.write_str("AuthHeader::Bearer { token: <redacted> }"),
            AuthHeader::CapabilityToken { .. } => f.write_str("AuthHeader::CapabilityToken { token: <redacted> }"),
            AuthHeader::CipherOctoCap { .. } => f.write_str("AuthHeader::CipherOctoCap { token: <redacted> }"),
        }
    }
}
```

#### `DispatchSet`

```rust
/// Per RFC-0969 §Data Structures.
pub struct DispatchSet {
    pub headers: Vec<AuthHeader>,
}

// Manual Debug redaction.
impl std::fmt::Debug for DispatchSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DispatchSet")
            .field("headers", &format_args!("<{} redacted headers>", self.headers.len()))
            .finish()
    }
}
```

#### `GatewayAuthenticator`

```rust
/// Per RFC-0969 §Roles.
pub struct GatewayAuthenticator {
    pub virtual_key_table: Arc<VirtualKeyTable>,        // RFC-0903
    pub holder_registry: Arc<dyn HolderRegistry>,       // RFC-0957-A1
    pub root_secret_lookup: Arc<dyn Fn(&[u8; 32]) -> Option<[u8; 32]>>,  // RFC-0957
    pub channel_providers: ChannelProviderSet,          // RFC-0957
    pub clock: Arc<dyn Clock>,                          // RFC-0957
}

impl GatewayAuthenticator {
    pub fn authenticate(&self, dispatch: &DispatchSet)
        -> Result<AuthenticatedIdentity, AuthError>;
}
```

#### `AuthenticatedIdentity`

```rust
pub struct AuthenticatedIdentity {
    /// Canonical holder DID (after identity linkage check).
    pub did: String,

    /// Holder Ed25519 public key (from HolderRegistry). R13-N4 fix: `Option<[u8;32]>`
    /// because the bearer-only path has no holder_pub; setting `[0u8;32]` would be an
    /// invalid Ed25519 point that leaks downstream.
    pub holder_pub: Option<[u8; 32]>,

    /// Ask binding (from capability, validated against bearer).
    pub ask_id: Option<[u8; 32]>,

    /// RFC-0903 verification result.
    pub bearer_verification: Option<BearerVerification>,

    /// RFC-0957 verification result.
    pub capability_verification: Option<CapabilityVerification>,
}

pub struct BearerVerification {
    pub virtual_key_id: String,
    pub subject_did: String,       // NEW: canonical identity for linkage check
    pub ask_id: Option<[u8; 32]>,  // NEW: ask binding for linkage check
    pub rate_limit_remaining: u64,
    pub budget_remaining_octows: u128,
}

pub struct CapabilityVerification {
    pub cap_root_hash: [u8; 32],
    pub caveats_satisfied: Vec<Caveat>,
    pub ask_id: Option<[u8; 32]>,
}
```

### Wire Format

This RFC does not introduce a new wire format. The wire is the HTTP request envelope itself. The dual-header request is:

```http
GET /v1/inference HTTP/1.1
Host: gateway.cipherocto
Authorization: Bearer sk-cipherocto-abc123
X-Capability-Token: eyJ...macaroon....eyJ...sig....eyJ...discharges
X-Request-Id: req_42
```

The bearer header value is the RFC-0903 virtual key string. The capability header value is the RFC-0957 3-segment wire string. The headers are independently optional; the AND-gate is enforced when both are present.

### Algorithms

#### `DispatchSet::from_headers()`

```rust
pub fn from_headers(headers: &http::HeaderMap) -> Result<Self, ParseError> {
    let mut auth_headers = Vec::new();
    let mut authorization_count = 0;
    let mut capability_count = 0;
    let mut x_capability_token_count = 0;

    for (name, value) in headers.iter() {
        let value_str = value.to_str().map_err(|_| ParseError::InvalidHeaderValue)?;
        let name_lower = name.as_str().to_lowercase();

        match name_lower.as_str() {
            "authorization" => {
                authorization_count += 1;
                if authorization_count > 1 {
                    return Err(ParseError::DuplicateAuthHeader);
                }
                if let Some(stripped) = value_str.strip_prefix("Bearer ") {
                    auth_headers.push(AuthHeader::Bearer { token: stripped.to_string() });
                } else if let Some(stripped) = value_str.strip_prefix("CipherOcto-Cap ") {
                    auth_headers.push(AuthHeader::CipherOctoCap { token: stripped.to_string() });
                } else {
                    return Err(ParseError::UnsupportedAuthScheme(value_str.to_string()));
                }
            }
            "x-capability-token" => {
                x_capability_token_count += 1;
                if x_capability_token_count > 1 {
                    return Err(ParseError::DuplicateCapabilityHeader);
                }
                auth_headers.push(AuthHeader::CapabilityToken { token: value_str.to_string() });
                capability_count += 1;
            }
            _ => {}
        }
    }

    if capability_count > 0 && auth_headers.iter().any(|h| matches!(h, AuthHeader::CipherOctoCap { .. })) {
        return Err(ParseError::DuplicateCapabilityHeader);
    }

    if auth_headers.is_empty() {
        return Err(ParseError::NoAuthHeader);
    }

    Ok(DispatchSet { headers: auth_headers })
}
```

#### `GatewayAuthenticator::authenticate()`

```rust
pub fn authenticate(&self, dispatch: &DispatchSet)
    -> Result<AuthenticatedIdentity, AuthError>
{
    let mut bearer_verification: Option<BearerVerification> = None;
    let mut capability_verification: Option<CapabilityVerification> = None;
    let mut capability_ask_id: Option<[u8; 32]> = None;     // Round 3 R2 M13 fix: flat type
    let mut capability_did: Option<String> = None;
    // R17-N1 fix: hoist active_holder_pub to function scope (was in match arm scope, then referenced outside the loop; unreachable for bearer-only path)
    let mut active_holder_pub: Option<[u8; 32]> = None;

    for header in &dispatch.headers {
        match header {
            AuthHeader::Bearer { token } => {
                let v = self.virtual_key_table.verify(token)?;
                bearer_verification = Some(v);
            }
            AuthHeader::CapabilityToken { token } | AuthHeader::CipherOctoCap { token } => {
                // Round 3 R2 C1 fix: extract macaroon's root_id from the wire's
                // first segment, then cap_root_hash = BLAKE3(macaroon.root_id).
                // The wire-only derivation matches the mint-time derivation
                // (Round 3 R2 C8: revoked records rejected by lookup_active).
                let macaroon = deserialize_macaroon_segment_1(token)?;
                let cap_root_hash = BLAKE3(&macaroon.root_id);
                let active = self.holder_registry.lookup_active(&cap_root_hash, &*self.clock)?
                    .ok_or(AuthError::UnknownHolderOrRevoked { cap_root_hash })?;
                // R15-N1 + R17-N1 fix: assign to function-scope binding (was in match arm scope, then referenced outside the loop)
                active_holder_pub = Some(active.holder_pub);

                let ctx = VerifyContext {
                    discharges: DischargeSet::default(),
                    channel_providers: self.channel_providers.clone(),
                    clock: self.clock.clone(),
                    root_secret_lookup: self.root_secret_lookup.clone(),
                    holder_registry: self.holder_registry.clone(),
                };
                let cap_token = deserialize_wire(token, &active.holder_did, &active.holder_pub)?;
                verify(&cap_token, &ctx)?;

                capability_verification = Some(CapabilityVerification {
                    cap_root_hash,
                    caveats_satisfied: cap_token.caveats(),
                    ask_id: cap_token.ask_binding(),
                });
                capability_ask_id = cap_token.ask_binding();        // Round 3 R2 M13 fix
                capability_did = Some(active.holder_did.clone());
            }
        }
    }

    if bearer_verification.is_none() && capability_verification.is_none() {
        return Err(AuthError::NoAuthHeader);
    }

    // AND-gate: identity linkage (flat types; round 3 R2 M13 fix).
    if let (Some(bv), Some(cv)) = (&bearer_verification, &capability_verification) {
        if let Some(cap_did) = &capability_did {
            if bv.subject_did != *cap_did {
                return Err(AuthError::IdentityMismatch {
                    bearer_did: bv.subject_did.clone(),
                    capability_did: cap_did.clone(),
                });
            }
        }
        // R15-N21 fix: prior guard `if let (Some(bearer_ask), Some(cap_ask)) = ...`
    // silently skipped the check when either side was None, allowing legacy
    // bearer-without-ask_id + capability-with-ask_id to pass (cross-ask attack).
    // Both sides must be Some; legacy bearer without ask_id is REJECTED, not
    // silently allowed.
    let bearer_ask = bv.ask_id.ok_or_else(|| AuthError::AskBindingMissing { field: "bearer" })?;
    let cap_ask = capability_ask_id.ok_or_else(|| AuthError::AskBindingMissing { field: "capability" })?;
    // R18-N7 + R19-N1 fix: outer ?-unwrapped lets above do the gating work; the if-let
    // block below uses the unwrapped values directly. R19-N1 fix: the prior dangling
    // closing braces after this block have been removed; the if-let block now closes
    // once and the outer brace pair is balanced.
    if bearer_ask != cap_ask {
        return Err(AuthError::AskBindingMismatch {
            bearer_ask,
            capability_ask: cap_ask,
        });
    }
    }  // R53-N1 fix: close the outer `if let (Some(bv), Some(cv))` block. The bearer_ask/cap_ask
       // checks are inside the if-let (they use `bv`); the lets at L419+ use only function-scope
       // vars and can live outside the if-let.

    let did = capability_did.unwrap_or_else(|| {
        // R12-N8 fix: prior format `did:octo:bearer:<virtual_key_id>` (R11-N7 was a comment-only
        // no-op) violates RFC-0009 §Identity Key Format. Use the real subject_did.
        // R30-N5 fix: comment continuation + body indented to column 8 to match surrounding code.
        bearer_verification.as_ref().unwrap().subject_did.clone()
    });
    let holder_pub = capability_verification.as_ref()
        .map(|c| active_holder_pub.ok_or(AuthError::HolderPubMissing { cap_root_hash: c.cap_root_hash })).transpose()?;  // R18-N6 fix: typed AuthError instead of .expect() panic
    let ask_id = capability_ask_id
        .or_else(|| bearer_verification.as_ref().and_then(|b| b.ask_id));  // R29-N4 fix:
                                                                            // the .or_else
                                                                            // branch is
                                                                            // reachable and
                                                                            // REQUIRED for the
                                                                            // bearer-only path
                                                                            // (TV1); the prior
                                                                            // R23-N8 was RESOLVED (R29-N4); the .or_else branch IS reached for bearer-only
                                                                            // marker was
                                                                            // wrong. The
                                                                            // `if let (Some(bv), Some(cv))`
                                                                            // gate at L392
                                                                            // SKIPS for
                                                                            // bearer-only,
                                                                            // which is the
                                                                            // primary use case
                                                                            // for the bearer
                                                                            // pipeline.
                                                                            // R37-N10 fix: DEFERRED marker dropped per [[deferred-vs-unspecified]]; R23-N8 item is RESOLVED.

    Ok(AuthenticatedIdentity {
        did,
        holder_pub,
        ask_id,
        bearer_verification,
        capability_verification,
    })
}
```

#### Header Precedence Rules

When multiple auth headers are present:

1. **`Authorization: Bearer` + `X-Capability-Token`** — both paths must succeed AND identity must match. AND-gate.
2. **`Authorization: Bearer` + `Authorization: CipherOcto-Cap`** — both paths must succeed; identity linkage.
3. **`X-Capability-Token` + `Authorization: CipherOcto-Cap`** — REJECT with `AuthError::DuplicateCapabilityHeader`. The two are aliases.
4. **Two `Authorization: Bearer` headers** — REJECT with `AuthError::DuplicateAuthHeader`.
5. **Two `X-Capability-Token` headers** — REJECT with `AuthError::DuplicateCapabilityHeader`.

#### `mint_dual` Algorithm

```rust
/// Per RFC-0969 §Algorithms.
/// Mint both tokens for the same holder; write both records to HolderRegistry
/// in a single transaction (RFC-0957-A1 §TransactionExt::insert_dual).
/// Round 3 R2 C2 + C3 + C6 fixes: persistence is via the transaction's
/// `insert_dual`; mint() is persistence-free; buyer's pubkey is explicit.
pub fn mint_dual(
    buyer_did: &str,
    buyer_holder_pub: &[u8; 32],                    // Round 3 R2 C3 fix
    ask_id: &[u8; 32],
    ask_ttl_unix: u64,
    // R22-N1 fix: REMOVED `bearer_root_secret` (was dead parameter; body never read it).
    capability_root_secret: &[u8; 32],
    buyer_encryption_pubkey: &X25519PublicKey,
    wallet: &dyn WalletCrypto,
    db: &stoolap::Database,                         // concrete substrate
) -> Result<(BearerCapsule, CapabilityToken), MintError> {
    // Step 1: Begin a single Stoolap transaction.
    let mut txn = db.begin()?;

    // Step 2: Mint the bearer capsule (Round 3 R2 C5 fix: defined in RFC-0959-A1
    // §mint_bearer_capsule, not in RFC-0903).
    let bearer = mint_bearer_capsule(
        buyer_did,
        ask_id,
        ask_ttl_unix,
        buyer_encryption_pubkey,
        wallet,
    )?;

    // Step 3: Mint the capability token (Round 3 R2 C2 fix: persistence-free).
    // The `holder` is the BUYER's pubkey (Round 3 R2 C3 fix), not the seller's.
    let capability = CapabilityToken::mint(
        capability_root_secret,
        &IdentityKey::from_public_bytes(buyer_holder_pub)?,  // R39-N1 fix: DEFERRED (R19-N9) — IdentityKey::from_public_bytes is a working stub in 0957-A1 L80 (no formal trait declaration); the 3 call sites (0957 stub, 0959 L520 — R60-N3 fix: was L498, shifted +22 by R58 Debug impl additions in 0959, 0969 L507 here — R54-N3 fix: was L504) all reference the same phantom method. R55-N4 fix: dropped the contradictory R51-N2 sentence (the "L504→L505 hallucination → L504 → L507 per R54-N3" chain was self-contradictory); the authoritative line is L507 per R54-N3.
        buyer_did.to_string(),
        vec![
            Caveat::BeforeMillis(ask_ttl_unix * 1000),    // R9-N5 fix: canonical name per RFC-0957-A1 §Caveat Variant Aliases (discriminant byte 0x04); millis
            Caveat::Audience(buyer_did.to_string()),
            Caveat::AskBinding(*ask_id),             // Round 3 R2 C4 fix
        ],
    )?;

    // Step 4: Build both HolderRecords.
    let bearer_record = HolderRecord::from_bearer(&bearer, buyer_holder_pub, buyer_did, *ask_id, ask_ttl_unix * 1000);  // R21-N2 fix: pass buyer_holder_pub (canonical signature per R20-N3)
    let cap_record = HolderRecord::from_capability(
        &capability,
        buyer_holder_pub,                     // R24-N2 fix: pass buyer_holder_pub as 2nd arg (R23-N2 canonical signature)
        buyer_did,
        Some(*ask_id),
        ask_ttl_unix * 1000,
    );

    // Step 5: Insert both records atomically via the transaction's insert_dual.
    // Round 3 R2 C6 fix: insert_dual is on TransactionExt, not on HolderRegistry
    // (which would open a separate transaction).
    txn.insert_dual(bearer_record, cap_record)?;

    // Step 6: Commit transaction.
    txn.commit()?;

    Ok((bearer, capability))
}
```

## Roles and Authorities

### Role/Authority Coverage Table

| Role | Identifier | Authority Scope | Lifecycle | Source/Ref |
|------|------------|-----------------|-----------|------------|
| Gateway Authenticator | per-gateway instance | parse + route + dispatch auth headers | stateless beyond local cache | RFC-0969 §Roles (NEW) |
| Virtual-Key Operator (RFC-0903) | RFC-0903 §Roles | bearer virtual-key management | operator lifecycle | RFC-0903 |
| Token Issuer (RFC-0957) | RFC-0009 `IdentityKey` of issuing node | mint + revoke + register | node identity lifecycle | RFC-0957 |
| Holder | RFC-0009 `IdentityKey` of holder | own DID + holder_pub + caveats | node identity lifecycle | RFC-0009 |
| Legacy Client | HTTP client (no identity key) | use bearer path | stateless | RFC-0903 |
| Capability Client | wallet-side agent | use capability path + sign Ed25519 | wallet-side lifecycle | RFC-0957 |

## Lifecycle Requirements

The Gateway Authenticator has minimal state (just the local cache). It does not have a state machine in the traditional sense.

### Gateway Authenticator Lifecycle

```mermaid
stateDiagram-v2
    [*] --> Ready: instance created
    Ready --> Authenticating: authenticate() called
    Authenticating --> Ready: result returned
    Ready --> [*]: instance dropped
```

### Recovery Semantics

On gateway restart: the Gateway Authenticator rebuilds its state from the local virtual-key table + the `HolderRegistry` snapshot.

### Time Bounds

- `authenticate()` MUST complete in ≤ 1ms p99 (G6).
- The HolderRegistry lookup dominates; bounded by RFC-0957-A1 §Performance.

## Determinism Requirements

- **`DispatchSet::from_headers()` ordering:** preserves header order.
- **`authenticate()` ordering:** iterates headers in order; AND-gate is symmetric.
- **`AuthenticatedIdentity.did` derivation:** from `HolderRegistry.holder_did` for capability path; from `BearerVerification.subject_did` (RFC-0903 virtual-key table) for bearer-only path. (R14-N5 fix: prior text said "synthesized from virtual-key ID" which violates RFC-0009 §Identity Key Format; the implementation at line 408-410 uses the actual `subject_did` per R12-N8.)
- **Header precedence rules:** deterministic; same input → same output.

### RFC-0008 Execution Class Mapping

| Operation | Class | Rationale |
|-----------|-------|-----------|
| `DispatchSet::from_headers()` | A | Pure deterministic parser |
| `authenticate()` RFC-0903 path | A | Virtual-key table lookup is deterministic |
| `authenticate()` RFC-0957 path | A | HolderRegistry lookup + chain check are deterministic |
| `authenticate()` AND-gate + identity linkage | A | Pure boolean |
| `mint_dual()` | A | Single transaction; deterministic |

## Error Handling

```rust
// R45-N4 fix: manual Debug impl redaction. UnsupportedAuthScheme(String) may carry
// raw credential material if the parser encountered a malformed scheme header;
// redact to a length-only marker.
#[derive(thiserror::Error)]
pub enum ParseError {
    #[error("invalid header value: not valid UTF-8")]
    InvalidHeaderValue,

    #[error("unsupported auth scheme: {0}")]
    UnsupportedAuthScheme(String),

    #[error("no auth header in request")]
    NoAuthHeader,

    #[error("duplicate Authorization header")]
    DuplicateAuthHeader,

    #[error("duplicate X-Capability-Token header")]
    DuplicateCapabilityHeader,
}

impl std::fmt::Debug for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidHeaderValue => f.write_str("InvalidHeaderValue"),
            Self::UnsupportedAuthScheme(_) => f.write_str("UnsupportedAuthScheme(<redacted: scheme string>)"),
            Self::NoAuthHeader => f.write_str("NoAuthHeader"),
            Self::DuplicateAuthHeader => f.write_str("DuplicateAuthHeader"),
            Self::DuplicateCapabilityHeader => f.write_str("DuplicateCapabilityHeader"),
        }
    }
}

/// R12-N4 fix: `MintError` was referenced throughout (lines 31, 452, 556, 772, 783)
/// but never defined. Added as a distinct enum from `AuthError` because mint-time
/// errors (key resolution, ask expiry, caveat rejection) are semantically different
/// from verify-time errors (header parsing, identity linkage).
// R44-N5 fix: manual Debug impl redaction. ask_id is credential-binding per RFC-0959.
#[derive(thiserror::Error)]
pub enum MintError {
    #[error("per-ask root secret not found for ask_id={:x?}", ask_id)]
    RootSecretNotFound { ask_id: [u8; 32] },

    #[error("stoolap database error: {0}")]
    StoolapDbError(#[from] stoolap::Error),  // R15-N9 fix: txn.begin()? returns stoolap::Error
    #[error("stoolap transaction error: {0}")]
    StoolapTxnError(#[from] stoolap::TxnError),  // R15-N9 fix
    #[error("registry error: {0}")]
    RegistryError(#[from] RegistryError),  // R15-N9 fix
    #[error("CAS error: {0}")]
    CasError(#[from] CasError),  // R15-N9 fix

    #[error("ask expired: ask_id={:x?}, ttl_unix={}, now_unix={}", ask_id, ttl_unix, now_unix)]
    AskExpired { ask_id: [u8; 32], ttl_unix: u64, now_unix: u64 },

    #[error("caveat rejected: {0}")]
    CaveatRejected(String),

    #[error("canonical serialization error: {0}")]
    SerializationError(#[from] CanonicalSerError),

    #[error("wallet error: {0}")]
    WalletError(String),

    #[error("identity error: {0}")]
    IdentityError(String),
}

impl std::fmt::Debug for MintError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RootSecretNotFound { .. } => f.write_str("RootSecretNotFound(<redacted: ask_id=32 bytes>)"),
            Self::StoolapDbError(_) => f.write_str("StoolapDbError(<redacted>)"),
            Self::StoolapTxnError(_) => f.write_str("StoolapTxnError(<redacted>)"),
            Self::RegistryError(_) => f.write_str("RegistryError(<redacted>)"),
            Self::CasError(_) => f.write_str("CasError(<redacted>)"),
            Self::AskExpired { ttl_unix, now_unix, .. } => write!(f, "AskExpired(<redacted: ask_id=32 bytes>, ttl_unix={}, now_unix={})", ttl_unix, now_unix),
            Self::CaveatRejected(_) => f.write_str("CaveatRejected(<redacted>)"),
            Self::SerializationError(_) => f.write_str("SerializationError(<redacted>)"),
            Self::WalletError(_) => f.write_str("WalletError(<redacted>)"),
            Self::IdentityError(_) => f.write_str("IdentityError(<redacted>)"),
        }
    }
}

// R44-N4 fix: manual Debug impl redaction. cap_root_hash/ask_id are credential-binding;
// bearer_did/capability_did may enable cross-correlation if leaked in Debug.
#[derive(thiserror::Error)]
pub enum AuthError {
    #[error("no auth header in request")]
    NoAuthHeader,

    #[error("bearer verification failed: {0}")]
    BearerVerificationFailed(#[from] VirtualKeyError),

    #[error("capability verification failed: {0}")]
    CapabilityVerificationFailed(#[from] VerifyError),

    #[error("unknown holder: cap_root_hash={:x?}", cap_root_hash)]
    UnknownHolder { cap_root_hash: [u8; 32] },

    #[error("unknown holder or revoked: cap_root_hash={:x?}", cap_root_hash)]
    UnknownHolderOrRevoked { cap_root_hash: [u8; 32] },  // R12-N3 fix: code at L362 returns `UnknownHolderOrRevoked` (variant was missing before R12-N3).
                                                          // R32-N8 fix: L358 was a comment line; actual return at L362.
                                                          // R35-N4 + R35-N10 fix: reworded 'pseudo-code at L362' to 'code at L362' (L362 is the REAL implementation, not pseudo-code).

    #[error("identity mismatch: bearer.subject_did={}, capability.holder_did={}", bearer_did, capability_did)]
    IdentityMismatch { bearer_did: String, capability_did: String },

    #[error("ask binding missing on {field} side")]
    AskBindingMissing { field: &'static str },  // R16-N1 fix: variant required by R15-N21 fix

    #[error("holder_pub missing for capability: cap_root_hash={:x?}", cap_root_hash)]
    HolderPubMissing { cap_root_hash: [u8; 32] },  // R18-N6 fix: typed error replaces .expect() panic

    #[error("ask binding mismatch: bearer.ask_id={:x?}, capability.ask_id={:x?}", bearer_ask, capability_ask)]
    AskBindingMismatch { bearer_ask: [u8; 32], capability_ask: [u8; 32] },
}

impl std::fmt::Debug for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoAuthHeader => f.write_str("NoAuthHeader"),
            Self::BearerVerificationFailed(_) => f.write_str("BearerVerificationFailed(<redacted>)"),
            Self::CapabilityVerificationFailed(_) => f.write_str("CapabilityVerificationFailed(<redacted>)"),
            Self::UnknownHolder { .. } => f.write_str("UnknownHolder(<redacted: cap_root_hash=32 bytes>)"),
            Self::UnknownHolderOrRevoked { .. } => f.write_str("UnknownHolderOrRevoked(<redacted: cap_root_hash=32 bytes>)"),
            Self::IdentityMismatch { .. } => f.write_str("IdentityMismatch(<redacted: bearer_did/capability_did strings>)"),
            Self::AskBindingMissing { field } => write!(f, "AskBindingMissing {{ field: {:?} }}", field),
            Self::HolderPubMissing { .. } => f.write_str("HolderPubMissing(<redacted: cap_root_hash=32 bytes>)"),
            Self::AskBindingMismatch { .. } => f.write_str("AskBindingMismatch(<redacted: bearer_ask/capability_ask 32 bytes>)"),
        }
    }
}
```

## Performance Targets

| Metric | Target | Notes |
|--------|--------|-------|
| Routing latency | ≤ 1ms p99 added | Header parser + dispatch |
| HolderRegistry lookup | ≤ 5ms p99 | RFC-0957-A1 §Performance |
| Bearer virtual-key lookup | ≤ 1ms p99 | RFC-0903 §Performance |
| Identity linkage check | ≤ 0.1ms p99 | String + Option compare |

## Security Considerations

### Threat Model Additions

- **Header smuggling** — attacker sends Bearer + Capability hoping one path bypasses the other. Mitigation: AND-gate.
- **Bearer-only downgrade** — attacker strips Capability. Mitigation: by design; bearer-only is the legacy path.
- **Cross-holder credential mixing** — attacker has Bearer for holder A and Capability for holder B; sends both. Mitigation: `IdentityMismatch` check forces `bearer.subject_did == capability.holder_did`.
- **Cross-ask credential mixing** — attacker has Bearer for ask X and Capability for ask Y. Mitigation: `AskBindingMismatch` check forces equality.
- **Capability header forgery** — attacker forges a Capability header. Mitigation: `UnknownHolder` error.
- **Holder registry poisoning** — see RFC-0957-A1 §Security.
- **Routing logic injection** — malformed `Authorization` header. Mitigation: `ParseError::UnsupportedAuthScheme` rejects.
- **Debug credential leak** — `format!("{:?}", dispatch)` would have leaked tokens. Mitigation: manual `impl Debug` redaction.

### Key Handling Rules

UNCHANGED from RFC-0957 §Key Handling Rules + RFC-0903 §Key Handling Rules.

### Cryptographic Agility

UNCHANGED from RFC-0957 + RFC-0903.

### Replay Protection

Capability path inherits RFC-0957 §Replay Protection. Bearer path inherits RFC-0903 §Replay Protection. AND-gate combines both.

## Adversary Analysis (5-Question Test)

### Finding A12: Header smuggling bypass

1. **Who benefits?** — Attacker with a valid bearer but no valid capability.
2. **What does it cost them?** — A valid bearer.
3. **What do they gain if successful?** — They could use the bearer path alone. This is the legitimate bearer path.
4. **What's our defense?** — AND-gate requires BOTH paths to succeed if BOTH headers are present. A valid bearer alone, with no capability, is rejected if the capability header is also present and invalid.
5. **What's the residual risk?** — Attacker can omit the capability header and use bearer alone. Legitimate bearer path.

Verdict: NO RISK. By design.

### Finding A13: Header collision (Bearer + CipherOcto-Cap same Authorization)

1. **Who benefits?** — Attacker trying to confuse the parser.
2. **What does it cost them?** — A malformed request.
3. **What do they gain if successful?** — Ambiguous routing; possible parser crash.
4. **What's our defense?** — Parser rejects multiple `Authorization` headers.
5. **What's the residual risk?** — None.

Verdict: NO RISK. Parser-side rejection.

### Finding A14: Routing latency DoS

1. **Who benefits?** — Attacker who wants to slow down the gateway.
2. **What does it cost them?** — A flood of requests.
3. **What do they gain if successful?** — Gateway latency spike.
4. **What's our defense?** — Rate limiting per RFC-0903 + capability token expiry. Header parsing is O(n) where n = header count.
5. **What's the residual risk?** — A flood of legitimate-looking requests. Standard DoS mitigation.

Verdict: ACCEPTED RISK. Standard DoS mitigation.

### Finding A21: Cross-holder credential mixing (Round 2 R2 C3)

1. **Who benefits?** — Attacker with valid bearer for holder A and valid capability for holder B.
2. **What does it cost them?** — Two valid credentials.
3. **What do they gain if successful?** — Cross-holder authorization; the audit trail attributes the request to holder A's bearer but the capability was actually holder B's.
4. **What's our defense?** — `IdentityMismatch` check forces `bearer.subject_did == capability.holder_did`.
5. **What's the residual risk?** — None; the check is mandatory when both paths are present.

Verdict: NO RISK. Mandatory AND-gate with identity linkage.

## Dependency Validation

| RFC# | Type | Current Status (2026-08-01) | Assumed Before Accept? | Hard-block on RFC-0969 acceptance? |
|------|------|------------------------------|------------------------|------------------------------------|
| RFC-0903 | Requires | Accepted | Already | No |
| RFC-0957 | Requires | Accepted | Already | No |
| RFC-0957-A1 | Requires | Draft | Yes | YES |
| RFC-0009 | Requires | Accepted | Already | No |
| RFC-0949 | Optional | Draft | Best-effort | No |
| RFC-0959-A1 | Optional | Draft | No | No |

**DAG check:** `0969 ← {0903, 0957, 0957-A1, 0009, 0949*, 0959-A1*}` — acyclic. Valid.

## Implicit Assumptions Audit

| Assumption | Where Relied Upon | Blast Radius if False | Mitigation / Status |
|------------|-------------------|----------------------|---------------------|
| **IA-1: Header order is preserved by the HTTP library** | §Algorithms `from_headers` | Routing determinism breaks | Use `http::HeaderMap::iter()` |
| **IA-2: Virtual-key table is operator-managed** | §Roles Virtual-Key Operator | Bearer path unavailable | RFC-0903 §Operations |
| **IA-3: HolderRegistry is local to the gateway** | §Algorithms HolderRegistry lookup | Cross-node capability verification fails | RFC-0957-A1 §HolderRegistry binding |
| **IA-4: AND-gate semantics are agreed upon by all implementers** | §Algorithms AND-gate | Inconsistent routing across gateway impls | Test: TV1-TV5 |
| **IA-5: Virtual-key `subject_did` is canonical and trustworthy** | §Algorithms identity linkage | Cross-holder attack if subject_did is forgeable | RFC-0903 virtual-key issuance policy |
| **IA-6: Capability ask_id is canonical (matches bearer ask_id byte-for-byte)** | §Algorithms ask_id linkage | Cross-ask attack if ask_ids are not canonical | Test: TV11 |
| **IA-7: `mint_dual` uses `txn.insert_dual` for atomic pair insert** | §Algorithms `mint_dual` | Double-insert → `AskAlreadyExists` | RFC-0957-A1 §TransactionExt |

## Compatibility

### Backward Compatibility

- **Legacy bearer requests:** continue working.
- **Existing capability requests:** continue working.
- **Existing 2-token requests (rare):** continue working with the new identity linkage.

### Forward Compatibility

- **New auth header:** future auth schemes can be added to `AuthHeader` enum.
- **New HolderRegistry backend:** future storage backends replace the `dyn HolderRegistry`.

## Test Vectors

### TV1: Bearer-Only Request

```
Input:
  Authorization: Bearer sk-cipherocto-abc123
  X-Request-Id: req_42
Expected: Ok(AuthenticatedIdentity { did: "did:octo:<multibase>", bearer_verification: Some(...), capability_verification: None, ask_id: bearer.ask_id })  // R12-N8 fix: did is `BearerVerification.subject_did` (RFC-0009 multibase), not `did:octo:bearer:<virtual_key_id>`.
```

### TV2: Capability-Only Request

```
Input:
  X-Capability-Token: <macaroon>
Expected: Ok(AuthenticatedIdentity { did: <from registry>, bearer_verification: None, capability_verification: Some(...) })
```

### TV3: Bearer + Capability Request (Both Valid, Linked Identity)

```
Input:
  Authorization: Bearer sk-cipherocto-abc123 (subject_did = "did:octo:buyer1", ask_id = H1)
  X-Capability-Token: <macaroon> (holder_did = "did:octo:buyer1", ask_id = H1)
Expected: Ok(AuthenticatedIdentity { did: "did:octo:buyer1", bearer_verification: Some(...), capability_verification: Some(...), ask_id: Some(H1) })
```

### TV4: Bearer + Capability Request (Capability Invalid)

```
Input:
  Authorization: Bearer sk-cipherocto-abc123
  X-Capability-Token: <invalid macaroon>
Expected: Err(AuthError::CapabilityVerificationFailed(...))
```

### TV5: Bearer + Capability Request (Identity Mismatch)

```
Input:
  Authorization: Bearer sk-cipherocto-abc123 (subject_did = "did:octo:buyer1")
  X-Capability-Token: <macaroon> (holder_did = "did:octo:buyer2")
Expected: Err(AuthError::IdentityMismatch { bearer_did: "did:octo:buyer1", capability_did: "did:octo:buyer2" })
```

### TV6: Duplicate Capability Header

```
Input:
  X-Capability-Token: <macaroon>
  Authorization: CipherOcto-Cap <different macaroon>
Expected: Err(ParseError::DuplicateCapabilityHeader)
```

### TV7: No Auth Header

```
Input: <no auth header>
Expected: Err(ParseError::NoAuthHeader)
```

### TV8: Unsupported Auth Scheme

```
Input:
  Authorization: Basic <base64>
Expected: Err(ParseError::UnsupportedAuthScheme("Basic ..."))
```

### TV9: Dual-Issuance Atomicity

```
Input: mint_dual("did:octo:buyer1", &H1, 1700086400, ...)
Expected: Ok((bearer, capability)) with both records in HolderRegistry
         If either insert fails, both roll back
```

### TV10: Debug Redaction

```
Action: format!("{:?}", dispatch)
Expected output: contains "AuthHeader::Bearer { token: <redacted> }"
Expected output: does NOT contain raw token bytes
```

### TV11: Ask Binding Mismatch

```
Input:
  Authorization: Bearer sk-cipherocto-abc123 (subject_did = "did:octo:buyer1", ask_id = H1)
  X-Capability-Token: <macaroon> (holder_did = "did:octo:buyer1", ask_id = H2)
Expected: Err(AuthError::AskBindingMismatch { bearer_ask: H1, capability_ask: H2 })
```

### TV12: Cross-Impl Routing Determinism

```
Input: 100 random request headers
Expected: routing decision byte-identical across two independent gateway implementations
```

## Alternatives Considered

| Approach | Pros | Cons | Verdict |
|----------|------|------|---------|
| **(a) Bearer-only gateway** | Simple | Wallet clients fail | Rejected |
| **(b) Capability-only gateway** | Clean | Legacy clients break | Rejected |
| **(c) Two separate gateways** | Clear separation | Operational overhead | Rejected |
| **(d) Single gateway, OR-gate** | Flexible | Downgrade attacks | Rejected |
| **(e) Single gateway, AND-gate with identity linkage (this RFC)** | Secure; consistent | Requires dual-issuance | **Adopted** |

## Upstream Dependencies (Round 3 R2 R1 C2/C5 fix)

This RFC depends on the following upstream amendments that MUST be in place before RFC-0969 reaches Accepted. None of these is in scope for this RFC; they are listed for traceability.

1. **RFC-0009-B1: `WalletCrypto` trait.** Defines the `WalletCrypto` trait + methods used by `mint_dual` and `authenticate` (`identity_key()`, `sign()`, `hop_root_secret()`, `channel_session_key()`, `next_hop_counter()`, `channel_id()`, `buyer_encryption_pubkey()`). Without this, the algorithms cannot typecheck.

2. **RFC-0903-B2: Virtual-Key Verify API.** Defines `VirtualKeyTable::verify(token) -> Result<BearerVerification, VirtualKeyError>`. `BearerVerification` MUST include `subject_did: String` and `ask_id: Option<[u8; 32]>` for the AND-gate identity linkage to function. Without this, the AND-gate check is impossible.

3. **RFC-0903-C1-b: Virtual-Key Generation.** Defines the long-lived virtual-key generation algorithm (referenced by RFC-0959-A1 §mint_bearer_capsule but not by RFC-0903 itself). The capsule mint calls the generator; the generator returns the virtual key bytes; the capsule is the encrypted delivery.

These three amendments are PRE-REQUISITE for RFC-0969.

## Implementation Phases

### Phase 1: Header Parser + Routing

- [ ] `crates/octo-wallet/src/capability/dispatch.rs` (NEW) — `AuthHeader`, `DispatchSet`, `from_headers`
- [ ] `crates/quota-router-core/src/gateway/authenticator.rs` (NEW) — `GatewayAuthenticator`, `authenticate`
- [ ] Unit tests: TV1-TV12

### Phase 2: Dual-Issuance + HolderRegistry Integration

- [ ] `crates/octo-wallet/src/capability/dual_issuance.rs` (NEW) — `mint_dual` algorithm
- [ ] Integration test: TV9

### Phase 3: Mission Decomposition

- [ ] `missions/open/0969-a-dual-pipeline-gateway.md` — gateway routing implementation
- [ ] `missions/open/0969-b-dual-issuance-mint.md` — dual-issuance + HolderRegistry binding

## Key Files to Modify

| File | Change |
|------|--------|
| `crates/octo-wallet/src/capability/dispatch.rs` (NEW) | AuthHeader + DispatchSet |
| `crates/quota-router-core/src/gateway/authenticator.rs` (NEW) | GatewayAuthenticator |
| `crates/octo-wallet/src/capability/dual_issuance.rs` (NEW) | mint_dual algorithm |

## Future Work

- **F1: Header scheme registry** — extensible scheme support
- **F2: Multi-region routing** — gateway nearest-replica HolderRegistry lookup
- **F3: SSO forward-compat** — RFC-0949 IdP routing integration
- **F4: Auth metric export** — Prometheus counters for routing decisions

## Rationale

Why this approach over alternatives?

The dual-mode workflow requires both bearer and capability tokens to be accepted at the same gateway with identity linkage. The substrate is RFC-0903 (bearer) + RFC-0957 (capability) + RFC-0957-A1 (catalog + insert_dual). The mechanism is a header parser + AND-gate + identity linkage check + unified catalog.

## Version History

| Version | Date       | Changes |
|---------|------------|---------|
| 1.0     | 2026-08-01 | Initial draft |
| 1.1     | 2026-08-01 | Round 2: identity linkage (bearer.subject_did == cap.holder_did, bearer.ask_id == cap.ask_id); BearerVerification.subject_did + ask_id fields; mint_dual uses txn.insert_dual for atomic pair insert; Debug redaction; mint_dual has explicit ask_ttl_unix parameter |
| 2026-08-02 | **Promoted to Accepted.** Multi-round adversarial review R28-R64 converged; 2 maintainer approvals (@mmacedoeu + @cipherocto) completed; no blocking objections. Status header updated; file moved via `git mv` to `rfcs/accepted/economics/`. Brace balance verified at `authenticate()` (R53-N1 fix); phantom `IdentityKey::from_public_bytes` call site at L507 properly DEFERRED to RFC-0957-A2; ParseError / MintError / AuthError all have manual redacting Debug impls; identity linkage rule (bearer.subject_did == cap.holder_did ∧ bearer.ask_id == cap.ask_id) is the canonical cross-holder credential mixing defense. |

## Related RFCs

- RFC-0903 — bearer path
- RFC-0949 — SSO forward-compat hook
- RFC-0957 — capability path
- RFC-0957-A1 — unified catalog + insert_dual + Transaction
- RFC-0959-A1 — delivery populates both records
- RFC-0917 — orthogonal concept
- RFC-0970 — per-hop auth for forwarding
- RFC-0971 — destination-node role consolidation

## Related Use Cases

- [Dual-Mode Authorization Workflow](../../../docs/use-cases/dual-mode-authorization-workflow.md)

## Related Research

- [Dual-Mode Workflow Gap Research](../../../docs/research/2026-08-01-dual-mode-workflow-gap-research.md) — R1-R5 convergence

## Related Missions

- Future: `missions/open/0969-a-dual-pipeline-gateway.md`
- Future: `missions/open/0969-b-dual-issuance-mint.md`

## Cross-Reference: Outgoing Edges

This RFC is a dependency of:
- RFC-0970 — per-hop envelope
- RFC-0971 — meta RFC

## Appendices

### A. Sample Walk-Through

A wallet-side client makes a request after receiving a `MarketDeliveryEnvelope`:

```http
GET /v1/inference HTTP/1.1
Host: gateway.cipherocto
Authorization: Bearer sk-cipherocto-abc123          (subject_did = "did:octo:buyer1", ask_id = H1)
X-Capability-Token: eyJ...macaroon....eyJ...sig....eyJ...discharges  (holder_did = "did:octo:buyer1", ask_id = H1)
X-Request-Id: req_42
```

Gateway Authenticator:
1. Parses both headers → `DispatchSet { headers: [Bearer, CapabilityToken] }`.
2. Bearer path: virtual-key table lookup → Ok; subject_did = "did:octo:buyer1"; ask_id = H1.
3. Capability path: HolderRegistry lookup → Ok; holder_did = "did:octo:buyer1"; ask_id = H1.
4. AND-gate + identity linkage: `bearer.subject_did == cap.holder_did` (both "did:octo:buyer1") ✓; `bearer.ask_id == cap.ask_id` (both H1) ✓.
5. Both pass → request forwarded.

### B. Why Not OR-Gate?

OR-gate (any path valid) is rejected because:
- A valid bearer + invalid capability would still be accepted (downgrade).
- A valid capability + invalid bearer would still be accepted (bearer header unused).
- The holder's intent (dual-token) is ambiguous.
- No identity linkage → cross-holder credential mixing is possible.

The AND-gate with identity linkage is the only consistent interpretation.

### C. Why Not Separate Gateways?

Two separate gateways rejected because:
- Operational overhead: two deployments, two auth metrics, two routing tables.
- Routing complexity: clients must choose the right gateway based on auth type.
- The destination node's `HolderRegistry` already backs both paths.
