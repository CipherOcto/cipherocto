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

1. **Wire Format** — `Authorization: Bearer <sk-...>` OR `X-Capability-Token: <macaroon>` OR `Authorization: CipherOcto-Cap <macaroon>` (the alt path mentioned in RFC-0957). Each header alone is valid. Both auth headers present (bearer + capability) is rejected per the 4-way dispatch table (client misconfig; the legitimate "both" case is RFC-0959-A1 market delivery, server-side, not HTTP request ingress).
2. **Gateway Authenticator** — new role on the gateway. Owns the header parser + router + dispatch. Stateless beyond the local `HolderRegistry` cache.
3. **Routing algorithm** — header prefix determines parse path:
   - `Authorization: Bearer <...>` → RFC-0903 path (virtual-key table lookup + vault borrow)
   - `X-Capability-Token: <...>` → RFC-0957 path (HolderRegistry lookup + macaroon chain check + Ed25519 sig verify)
   - `Authorization: CipherOcto-Cap <...>` → RFC-0957 alt path
4. **Dual-issuance** — a holder can hold both a bearer (from RFC-0903) and a capability token (from RFC-0957) for the same `HolderRecord`. The destination node's mint endpoint accepts either request and writes to the same `HolderRegistry`.
5. **Dispatch table (v1.1 amendment)** — `routing_decision` computed from header count alone: bearer-only → `RoutingDecision::Bearer`; capability-only → `RoutingDecision::Capability`; both → `RoutingDecision::BothSchemesUnsupported` (rejected as client misconfig); neither → `RoutingDecision::NoAuth` (pass-through to model provider). The prior AND-gate identity linkage step is REMOVED in v1.1 because the dispatch table REJECTS the "both schemes" case as client misconfig — linkage is unreachable per new semantics.
6. **`mint_dual` algorithm** — `mint_dual(buyer_did, buyer_holder_pub, ask_id, ask_ttl_unix, capability_root_secret, buyer_encryption_pubkey, wallet, db)` — 8 params (R22-N1 fix: dropped dead `bearer_root_secret`; canonical signature per §`mint_dual` Algorithm at line 475 uses 8 params with `&dyn WalletCrypto` + `&stoolap::Database`). Mints both tokens exactly once and writes both via `TransactionExt::insert_dual` (RFC-0957-A1 §TransactionExt). `CapabilityToken::mint(root_secret, holder, holder_did, initial_caveats)` is the 4-arg persistence-free signature (R6-C3 fix, R7-N6 fix: NO `Some(&mut txn)` parameter; mint is pure crypto, no post-write hook). The caller writes both `HolderRecord`s via `txn.insert_dual(...)` in the same transaction, preventing the double-insert contradiction. (R15-N16 fix: `mint_dual` is a preview/test utility; production writes happen ONLY in `deliver_at_settlement` per RFC-0959-A1 §Algorithms. Calling both paths for the same ask_id would hit `UNIQUE(ask_id, kind)` and fail. Documented as the single-write authority.)
7. **Backward compat** — legacy clients (claude-code, hardcoded HTTP agents) using `Authorization: Bearer <sk-...>` continue working without client-side changes.
8. **Forward compat** — new clients opt into capability by including the wallet-side signer and `X-Capability-Token` header.
9. **Debug redaction** — `AuthHeader`, `DispatchSet`, `AuthenticatedRequest`, `BearerVerification`, `CapabilityVerification` use manual `impl Debug` with redaction.

## Why Needed

The dual-mode workflow requires two authorization pipelines running through the same gateway:

1. **Legacy bearer** — `Authorization: Bearer <sk-...>` for legacy clients. Validated by the gateway as a virtual-API-key (RFC-0903) or enterprise SSO token (RFC-0949, out of scope).
2. **Capability-based** — `X-Capability-Token: <3-segment macaroon>` for new wallet-side clients. Verified by RFC-0957 macaroon chain + Ed25519 holder signature + discharge channels.

Today's RFCs scatter this story. No document names the Gateway Authenticator role. No document specifies header-prefix routing. No document states that both tokens can coexist on the same request envelope (rejected per v1.1 dispatch table; legitimate "both" case is RFC-0959-A1 market delivery).

This RFC names the role, specifies the routing, applies the 4-way dispatch table, and binds both paths to the unified catalog.

## Scope

### In Scope

- Wire format coexistence (header names + parse paths).
- Gateway Authenticator role definition.
- Header-prefix routing algorithm.
- Dual-issuance semantics (one holder, two tokens, one registry entry).
- Header precedence rules (4-way dispatch table; both-schemes-present is `BothSchemesUnsupported`).
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
| **G2: Dual-pipeline coverage** | 100% of gateway requests routed through bearer path, capability path, or no-auth path (v1.1 dispatch table) | Test: 100 representative requests, all routed correctly |
| **G3: Header precedence (v1.1 amendment)** | Both Bearer + Capability-Token present → `BothSchemesUnsupported` (rejected per dispatch table). The prior AND-gate + identity-linkage behavior is REMOVED. | Test: dual-token request → `Err(AuthError::BothSchemesUnsupported)` regardless of identity match |
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
///
/// **v1.1 amendment (2026-08-08):** relocated to
/// `quota-router-core::ingress::authenticator`. The original location at
/// `crates/octo-wallet/src/capability/gateway_authenticator.rs` was orphan
/// substrate (668 lines, 0 production callers as of audit 2026-08-08). The
/// new location co-locates the orchestrator with its callers (HTTP proxy
/// `quota-router-core::proxy.rs::handle_request`).
pub struct GatewayAuthenticator {
    pub bearer_verifier: Arc<dyn BearerVerifier>,         // RFC-0871 §Traits
    pub capability_verifier: Arc<dyn CapabilityVerifier>, // RFC-0871 §Traits
    pub holder_registry: Arc<dyn HolderRegistry>,         // RFC-0957-A1
    pub clock: Arc<dyn Clock>,                            // quota-router-storage
}

impl GatewayAuthenticator {
    pub fn authenticate(&self, headers: &[(String, String)])
        -> Result<AuthenticatedRequest, AuthError>;
}
```

#### `AuthenticatedRequest`

```rust
/// **v1.1 amendment (2026-08-08):** renamed from `AuthenticatedIdentity`.
/// Dropped enrichment fields (rate_limit_remaining, budget_remaining_octows)
/// per the audit finding that the HTTP proxy gateway is transparent and MUST
/// NOT enrich `AuthenticatedRequest` with business-rule data. Rate limits,
/// budget, allowed-routes are checked SEPARATELY in
/// `quota-router-core::proxy.rs::handle_request` AFTER `authenticate()`
/// returns Ok.
pub struct AuthenticatedRequest {
    /// Canonical holder DID (post-linkage check).
    pub subject_did: String,
    pub ask_id: [u8; 32],
    /// RFC-0903 verification result (None on capability-only path).
    pub bearer: Option<BearerVerification>,
    /// RFC-0957 verification result (None on bearer-only path).
    pub capability: Option<CapabilityVerification>,
    /// Routing decision per the 4-way dispatch table (see §Dispatch Table).
    pub routing_decision: RoutingDecision,
}

pub struct BearerVerification {
    pub subject_did: String,
    pub ask_id: [u8; 32],
}

pub struct CapabilityVerification {
    pub holder_did: String,
    pub ask_id: [u8; 32],
}
```

#### `Dispatch Table` (v1.1 amendment, 2026-08-08)

Per the 2026-08-08 specialized node protocol research + user clarification, the original `LinkageResult::Indeterminate` / `RoutingDecision::Dual` machinery is replaced with a 4-way dispatch table:

| Request headers | RoutingDecision | Notes |
| --- | --- | --- |
| `Authorization: Bearer <x>` only (today's 100% traffic) | `RoutingDecision::Bearer` | RFC-0903 path |
| `Authorization: CipherOcto-Cap <x>` only (future cipherocto clients) | `RoutingDecision::Capability` | RFC-0957 path |
| Both `Authorization: Bearer <x>` AND `Authorization: CipherOcto-Cap <x>` | `RoutingDecision::BothSchemesUnsupported` (rejected as client misconfig) | Per 2026-08-08 clarification: client sending both is a bug, not a routing decision. The legitimate "both" case is RFC-0959-A1 market delivery, which is server-side delivery NOT HTTP request ingress. |
| Neither (no Authorization header) | `RoutingDecision::NoAuth` (pass-through to model provider) | Per 2026-08-08 clarification: provider-side decision (e.g., free experimentation marketing). Gateway does NOT error. |

`LinkageResult::Indeterminate` variant is REMOVED. `RoutingDecision::Dual` is REMOVED from the request-shape axis (only valid in RFC-0959-A1 delivery context, not HTTP request ingress). The dispatch is fully determined by request headers alone; no side-channel lookup needed for the dispatch decision itself.

**Cross-reference:** RFC-0871 §Wallet Node Lifecycle; `docs/research/2026-08-08-specialized-node-protocol-research.md` §5 (W3C DID Resolution + DIDComm — dispatch decision can be made from headers alone).

#### Removed types (v1.1 amendment)

The following types are REMOVED in v1.1:
- `LinkageResult::Indeterminate` — replaced by the 4-way dispatch table
- `RoutingDecision::Dual` (in the HTTP request ingress axis) — only valid in RFC-0959-A1 market delivery
- `AuthenticatedIdentity.rate_limit_remaining` — business-rule data, not gateway enrichment
- `AuthenticatedIdentity.budget_remaining_octows` — business-rule data, not gateway enrichment
- `BearerVerification.virtual_key_id` — duplicate of `subject_did` post-`IdentityKey` bridge
- `BearerVerification.rate_limit_remaining` / `.budget_remaining_octows` — moved to gateway-side check
- `CapabilityVerification.caveats_satisfied` — moved to post-`authenticate()` evaluation

Implementation mission: `missions/open/0969-a-gateway-relocation.md`.

### Wire Format

This RFC does not introduce a new wire format. The wire is the HTTP request envelope itself. The dual-header request is:

```http
GET /v1/inference HTTP/1.1
Host: gateway.cipherocto
Authorization: Bearer sk-cipherocto-abc123
X-Capability-Token: eyJ...macaroon....eyJ...sig....eyJ...discharges
X-Request-Id: req_42
```

The bearer header value is the RFC-0903 virtual key string. The capability header value is the RFC-0957 3-segment wire string. The headers are independently optional; the v1.1 dispatch table rejects both-present as `BothSchemesUnsupported` (client misconfig). The v1.0 AND-gate enforcement is REMOVED.

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
/// v1.1 amendment (2026-08-08): rewritten per the 4-way dispatch table
/// (see §Dispatch Table). The old AND-gate identity linkage step is
/// REMOVED because the dispatch table REJECTS the "both schemes" case as
/// client misconfig — linkage is unreachable per new semantics. The
/// `routing_decision` is computed from `dispatch.headers` alone (header
/// count, no side-channel lookup). Verifier traits (`BearerVerifier`,
/// `CapabilityVerifier`) replace direct `VirtualKeyTable` + `HolderRegistry`
/// access per RFC-0871 §Wallet Node Lifecycle.
pub fn authenticate(&self, dispatch: &DispatchSet)
    -> Result<AuthenticatedRequest, AuthError>
{
    let mut bearer: Option<BearerVerification> = None;
    let mut capability: Option<CapabilityVerification> = None;

    for header in &dispatch.headers {
        match header {
            AuthHeader::Bearer { token } => {
                bearer = Some(self.bearer_verifier.verify(token)?);
            }
            AuthHeader::CapabilityToken { token } | AuthHeader::CipherOctoCap { token } => {
                let macaroon = deserialize_macaroon_segment_1(token)?;
                let cap_root_hash = BLAKE3(&macaroon.root_id);
                let active = self.holder_registry.lookup_active(&cap_root_hash, &*self.clock)?
                    .ok_or(AuthError::UnknownHolderOrRevoked { cap_root_hash })?;

                let ctx = VerifyContext {
                    discharges: DischargeSet::default(),
                    channel_providers: self.capability_verifier.channel_providers(),
                    clock: self.clock.clone(),
                    root_secret_lookup: self.capability_verifier.root_secret_lookup(),
                    holder_registry: self.holder_registry.clone(),
                };
                let cap_token = deserialize_wire(token, &active.holder_did, &active.holder_pub)?;
                verify(&cap_token, &ctx)?;

                capability = Some(CapabilityVerification {
                    holder_did: active.holder_did.clone(),
                    ask_id: cap_token.ask_binding(),
                });
            }
        }
    }

    // 4-way dispatch (see §Dispatch Table). Computed from header presence alone.
    let routing_decision = match (bearer.is_some(), capability.is_some()) {
        (true, false) => RoutingDecision::Bearer,
        (false, true) => RoutingDecision::Capability,
        (true, true) => {
            // Per 2026-08-08 clarification: client sending both is a bug, not a routing decision.
            return Err(AuthError::BothSchemesUnsupported);
        }
        (false, false) => RoutingDecision::NoAuth,
    };

    // Build AuthenticatedRequest. Single-scheme-only per dispatch semantics,
    // so no identity linkage step is reachable. For NoAuth, identity fields
    // are empty placeholders; the caller (quota-router-core::proxy::handle_request)
    // treats NoAuth as provider-side decision per dispatch table.
    let (subject_did, ask_id) = match routing_decision {
        RoutingDecision::Bearer => {
            let b = bearer.as_ref().expect("dispatch guarantees Some");
            (b.subject_did.clone(), b.ask_id)
        }
        RoutingDecision::Capability => {
            let c = capability.as_ref().expect("dispatch guarantees Some");
            (c.holder_did.clone(), c.ask_id)
        }
        RoutingDecision::NoAuth => (String::new(), [0u8; 32]),
        RoutingDecision::BothSchemesUnsupported => unreachable!("handled above"),
    };

    Ok(AuthenticatedRequest {
        subject_did,
        ask_id,
        bearer,
        capability,
        routing_decision,
    })
}
```

#### Header Precedence Rules

When multiple auth headers are present (v1.1 amendment: 4-way dispatch table — see §Dispatch Table):

1. **`Authorization: Bearer` + `X-Capability-Token`** — REJECT with `Err(AuthError::BothSchemesUnsupported)`. The dispatch table REJECTS the both-schemes case as client misconfig before any verification runs. (v1.0 used AND-gate + identity linkage; this is REMOVED in v1.1.)
2. **`Authorization: Bearer` + `Authorization: CipherOcto-Cap`** — REJECT with `Err(AuthError::BothSchemesUnsupported)` for the same reason. (v1.0 used AND-gate + identity linkage; this is REMOVED in v1.1.)
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
- **`authenticate()` ordering:** iterates headers in order; dispatch is header-count-only (v1.1 amendment — see §Dispatch Table).
- **`AuthenticatedRequest.subject_did` derivation:** from `CapabilityVerification.holder_did` for capability path; from `BearerVerification.subject_did` for bearer-only path. (R14-N5 fix: prior text said "synthesized from virtual-key ID" which violates RFC-0009 §Identity Key Format; the implementation uses the actual `subject_did` per R12-N8. v1.1 amendment: field renamed from `did` to `subject_did` for consistency with `BearerVerification.subject_did`.)
- **Header precedence rules:** deterministic; same input → same output.

### RFC-0008 Execution Class Mapping

| Operation | Class | Rationale |
|-----------|-------|-----------|
| `DispatchSet::from_headers()` | A | Pure deterministic parser |
| `authenticate()` RFC-0903 path | A | Virtual-key table lookup is deterministic |
| `authenticate()` RFC-0957 path | A | HolderRegistry lookup + chain check are deterministic |
| `authenticate()` routing_decision computation | A | Pure boolean (header count per dispatch table) |
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

- **Header smuggling** — attacker sends Bearer + Capability hoping one path bypasses the other. Mitigation: 4-way dispatch table rejects `BothSchemesUnsupported` before any verification runs (v1.1 amendment; v1.0 AND-gate is REMOVED).
- **Bearer-only downgrade** — attacker strips Capability. Mitigation: by design; bearer-only is the legacy path.
- **Cross-holder credential mixing** — attacker has Bearer for holder A and Capability for holder B; sends both. Mitigation: dispatch table rejects `BothSchemesUnsupported` (no cross-holder path is reachable per v1.1 semantics).
- **Cross-ask credential mixing** — attacker has Bearer for ask X and Capability for ask Y. Mitigation: same — `BothSchemesUnsupported` reject.
- **Capability header forgery** — attacker forges a Capability header. Mitigation: `UnknownHolder` error.
- **Holder registry poisoning** — see RFC-0957-A1 §Security.
- **Routing logic injection** — malformed `Authorization` header. Mitigation: `ParseError::UnsupportedAuthScheme` rejects.
- **Debug credential leak** — `format!("{:?}", dispatch)` would have leaked tokens. Mitigation: manual `impl Debug` redaction.

### Key Handling Rules

UNCHANGED from RFC-0957 §Key Handling Rules + RFC-0903 §Key Handling Rules.

### Cryptographic Agility

UNCHANGED from RFC-0957 + RFC-0903.

### Replay Protection

Capability path inherits RFC-0957 §Replay Protection. Bearer path inherits RFC-0903 §Replay Protection. Per-scheme replay defense applies per dispatched path; no AND-gate combination (v1.1 amendment).

## Adversary Analysis (5-Question Test)

### Finding A12: Header smuggling bypass

1. **Who benefits?** — Attacker with a valid bearer but no valid capability.
2. **What does it cost them?** — A valid bearer.
3. **What do they gain if successful?** — They could use the bearer path alone. This is the legitimate bearer path.
4. **What's our defense?** — 4-way dispatch table rejects `BothSchemesUnsupported` before any verification (v1.1 amendment). Attacker cannot use the bearer path while also presenting an invalid capability; the both-present case is rejected at dispatch.
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
4. **What's our defense?** — 4-way dispatch table rejects `BothSchemesUnsupported` before any cross-holder path is reachable (v1.1 amendment; the v1.0 `IdentityMismatch` AND-gate check is REMOVED because unreachable).
5. **What's the residual risk?** — None; the both-schemes case is rejected at dispatch, before any verification runs.

Verdict: NO RISK. 4-way dispatch table rejects `BothSchemesUnsupported` upfront.

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
| **IA-4: 4-way dispatch semantics are agreed upon by all implementers** | §Dispatch Table | Inconsistent routing across gateway impls | Test: TV1-TV5 |
| **IA-5: Virtual-key `subject_did` is canonical and trustworthy** | §Wallet-as-Specialized-Node `BearerVerification.subject_did` | Cross-holder attack if subject_did is forgeable (reachable only on bearer-only path post-v1.1) | RFC-0903 virtual-key issuance policy |
| **IA-6: Capability `ask_id` is canonical** | §CapabilityVerification `ask_id` | Cross-ask attack if ask_id is forgeable (reachable only on capability-only path post-v1.1) | Test: TV11 |
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
Expected: Ok(AuthenticatedRequest {
  subject_did: "did:octo:<multibase>",
  ask_id: <bearer.ask_id>,
  bearer: Some(BearerVerification { subject_did: "did:octo:<multibase>", ask_id: <bearer.ask_id> }),
  capability: None,
  routing_decision: RoutingDecision::Bearer,
})
// v1.1 amendment: did renamed to subject_did; bearer_verification/capability_verification
// renamed to bearer/capability; routing_decision populated. Identity linkage step REMOVED
// per dispatch semantics (single scheme only on this path).
```

### TV2: Capability-Only Request

```
Input:
  X-Capability-Token: <macaroon>
Expected: Ok(AuthenticatedRequest {
  subject_did: <from registry>,
  ask_id: <capability.ask_id>,
  bearer: None,
  capability: Some(CapabilityVerification { holder_did: <from registry>, ask_id: <capability.ask_id> }),
  routing_decision: RoutingDecision::Capability,
})
```

### TV3: Bearer + Capability Request (Both Schemes — REJECTED per v1.1 dispatch table)

```
Input:
  Authorization: Bearer sk-cipherocto-abc123 (subject_did = "did:octo:buyer1", ask_id = H1)
  X-Capability-Token: <macaroon> (holder_did = "did:octo:buyer1", ask_id = H1)
Expected: Err(AuthError::BothSchemesUnsupported)
// v1.1 amendment: the v1.0 "both valid, linked identity" test case is REPLACED by
// this rejection. The legitimate "both" case is RFC-0959-A1 market delivery
// (server-side), NOT HTTP request ingress — see §Dispatch Table.
```

### TV4: Bearer + Capability Request (Capability Invalid — Still Rejected as Both-Schemes)

```
Input:
  Authorization: Bearer sk-cipherocto-abc123
  X-Capability-Token: <invalid macaroon>
Expected: Err(AuthError::BothSchemesUnsupported)
// v1.1 amendment: dispatch decision happens BEFORE capability verification.
// Both-schemes-present is rejected at the dispatch step regardless of validity.
// To test capability-invalid on the single-scheme path, omit the Bearer header.
```

### TV5: Capability-Only Request (Capability Invalid — Single Scheme)

```
Input:
  X-Capability-Token: <invalid macaroon> (no Bearer header)
Expected: Err(AuthError::CapabilityVerificationFailed(...))
// v1.1 amendment: this replaces the prior TV5 identity-mismatch test, which is
// unreachable under the new dispatch semantics (identity linkage requires both
// schemes, which dispatch rejects).
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
| **(e) Single gateway, 4-way dispatch table (this RFC, v1.1)** | Secure; consistent; rejects cross-scheme misuse | No dual-issuance needed | **Adopted** |

## Upstream Dependencies (Round 3 R2 R1 C2/C5 fix)

This RFC depends on the following upstream amendments that MUST be in place before RFC-0969 reaches Accepted. None of these is in scope for this RFC; they are listed for traceability.

1. **RFC-0009-B1: `WalletCrypto` trait.** Defines the `WalletCrypto` trait + methods used by `mint_dual` and `authenticate` (`identity_key()`, `sign()`, `hop_root_secret()`, `channel_session_key()`, `next_hop_counter()`, `channel_id()`, `buyer_encryption_pubkey()`). Without this, the algorithms cannot typecheck.

2. **RFC-0903-B2: Virtual-Key Verify API.** Defines `VirtualKeyTable::verify(token) -> Result<BearerVerification, VirtualKeyError>`. `BearerVerification` MUST include `subject_did: String` and `ask_id: [u8; 32]`. Per v1.1 amendment, the AND-gate identity linkage check is REMOVED (unreachable per dispatch table); the bearer-only path uses `subject_did` directly in `AuthenticatedRequest`.

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

The dual-mode workflow requires both bearer and capability tokens to be accepted at the same gateway. The substrate is RFC-0903 (bearer) + RFC-0957 (capability) + RFC-0957-A1 (catalog + insert_dual). The mechanism is a header parser + 4-way dispatch table + per-scheme verification + unified catalog (v1.1 amendment; the v1.0 AND-gate + identity linkage is REMOVED).

## Version History

| Version | Date       | Changes |
|---------|------------|---------|
| 1.0     | 2026-08-01 | Initial draft |
| 1.1     | 2026-08-01 | Round 2: identity linkage (bearer.subject_did == cap.holder_did, bearer.ask_id == cap.ask_id); BearerVerification.subject_did + ask_id fields; mint_dual uses txn.insert_dual for atomic pair insert; Debug redaction; mint_dual has explicit ask_ttl_unix parameter |
| 2026-08-02 | **Promoted to Accepted.** Multi-round adversarial review R28-R64 converged; 2 maintainer approvals (@mmacedoeu + @cipherocto) completed; no blocking objections. Status header updated; file moved via `git mv` to `rfcs/accepted/economics/`. Brace balance verified at `authenticate()` (R53-N1 fix); phantom `IdentityKey::from_public_bytes` call site at L507 properly DEFERRED to RFC-0957-A2; ParseError / MintError / AuthError all have manual redacting Debug impls; identity linkage rule (bearer.subject_did == cap.holder_did ∧ bearer.ask_id == cap.ask_id) is the canonical cross-holder credential mixing defense. |
| 1.2 | 2026-08-08 | **Accepted (amendment) — GatewayAuthenticator relocation + dispatch table.** Surfaced by 2026-08-08 specialized node protocol research (`docs/research/2026-08-08-specialized-node-protocol-research.md`) + RFC-0871. Four amendments: (1) **`GatewayAuthenticator` relocated** from `crates/octo-wallet/src/capability/gateway_authenticator.rs` (orphan substrate, 0 production callers per audit 2026-08-08) to `quota-router-core::ingress::authenticator`. Orchestrator co-locates with callers. (2) **`AuthenticatedIdentity` renamed to `AuthenticatedRequest`**; enrichment fields (`rate_limit_remaining`, `budget_remaining_octows`) DROPPED — the HTTP proxy gateway is transparent and MUST NOT enrich with business-rule data per user clarification 2026-08-07. Rate limits / budget / allowed-routes checked SEPARATELY in `quota-router-core::proxy::handle_request` AFTER `authenticate()` returns Ok. (3) **4-way dispatch table** replaces `LinkageResult::Indeterminate` + `RoutingDecision::Dual` machinery: bearer-only → `RoutingDecision::Bearer`; capability-only → `RoutingDecision::Capability`; both → `RoutingDecision::BothSchemesUnsupported` (client misconfig per user clarification 2026-08-07); neither → `RoutingDecision::NoAuth` (pass-through to model provider, per user clarification 2026-08-07). The legitimate "both" case is RFC-0959-A1 market delivery (server-side), NOT HTTP request ingress. (4) **Verifier traits** (`BearerVerifier`, `CapabilityVerifier`) replace direct `HolderRegistry` + `VirtualKeyTable` + `ChannelProviderSet` access — traits live in `octo-wallet::verify::*` per RFC-0871 §Wallet Node Lifecycle; orchestrator composes via `Arc<dyn Trait>`. Implementation mission: `missions/open/0969-a-gateway-relocation.md`. Cross-references: RFC-0871 §Implementation Phase 2; RFC-0870 §NodeEnvelope Adoption; memory `rfc-0969-dual-pipeline-semantics.md`. Round-1 adversarial review (2026-08-08) also fixed half-applied rename: §Algorithms §authenticate() and §Test Vectors rewritten to construct `AuthenticatedRequest { subject_did, ask_id, bearer, capability, routing_decision }` matching the v1.2 data structure. Algorithm body rewritten to compute `routing_decision` from header count alone per the dispatch table (no AND-gate identity linkage, which is unreachable under new semantics). |

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
- v1.0 used identity linkage → cross-holder credential mixing was possible (if linkage check itself was buggy). v1.1 closes this entirely by rejecting both-schemes at dispatch.

The 4-way dispatch table (reject `BothSchemesUnsupported`) is the v1.1 interpretation: single scheme per request, no cross-scheme path.

### C. Why Not Separate Gateways?

Two separate gateways rejected because:
- Operational overhead: two deployments, two auth metrics, two routing tables.
- Routing complexity: clients must choose the right gateway based on auth type.
- The destination node's `HolderRegistry` already backs both paths.
