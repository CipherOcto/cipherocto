# Research: Dual-Mode Workflow (Legacy Bearer + Capability) — Gap Inventory

**Date:** 2026-08-01
**Status:** Research → Use Case → RFC cluster (multiple RFCs needed)
**Author:** @cipherocto + @mmacedoeu
**Scope:** Map the existing RFCs + missions against the dual-mode authorization workflow (legacy bearer + capability-based) over the cipherocto forwarding network, identify the spec-level gaps, classify each as gap-in-scope-of-existing-RFC, in-place amendment, or new RFC. Verify the "remove holder_did from the empty-field deserialized struct" claim against current RFC-0957.

> **Format note:** This follows `docs/BLUEPRINT.md` research report template.

---

## Executive Summary

The dual-mode workflow has **two authorization pipelines** running in parallel through the same cipherocto forwarding network:

1. **Legacy bearer** — `Authorization: Bearer <sk-...>` for legacy clients (claude-code, hardcoded agent callers, anything written before the capability substrate). Validated by the gateway as a virtual-API-key (RFC-0903) OR an enterprise SSO token (RFC-0949).
2. **Capability-based** — `X-Capability-Token: <3-segment macaroon>` for new wallet-side clients. Verified by RFC-0957 macaroon chain + Ed25519 holder signature + discharge channels.

The **destination node** (the one holding the provider key) is the same node for both paths: it mints the bearer and/or the capability token, holds the catalog map (DID → holder_pub → caveats), and runs the egress transform before forwarding to the upstream provider.

The **forwarding hops** between source and destination are RFC-0870 `ForwardRequest` envelopes. Today the spec scope stops at "destination selection + local dispatch OR forward-to-peer". It does NOT pin the auth strategy these envelopes carry: bearer? capability? both? mint-at-each-hop? mint-at-destination?

Six concrete gaps are spec-level (not implementation gaps):

| # | Gap | Surface | Where it hurts |
|---|-----|---------|----------------|
| G1 | **No spec for dual-pipeline auth at forwarding hops** | RFC-0870 | Intermediate router nodes receive one envelope; they don't know whether to inspect or forward |
| G2 | **No spec for legacy bearer emission alongside capability token** | RFC-0903 + RFC-0957 | Market delivery cannot hand the buyer both tokens atomically |
| G3 | **Holder_did resolution strategy is unspecified** | RFC-0957 (out-of-band), RFC-0009 (vault) | Mint + verify both have a half-impl because the resolver is not designed |
| G4 | **`CapabilityHandle.holder_did` is structurally dead** | `crates/quota-router-core/src/egress.rs:199` | Constructors use `String::new()`; no producer populates the field |
| G5 | **No spec for "destination node is the mints"** | RFC-0870 + RFC-0957 + RFC-0959 | No role-binding between node-as-router and node-as-issuer |
| G6 | **No spec for market delivery envelope (both tokens at deal time)** | RFC-0959 + RFC-0955 | Settlement chain completes but no delivery artifact; buyer can't use what they bought |

The user's framing — *"the `holder_did` we can remove from the struct holding the empty field when deserialized, since it is empty we can get rid of it. The mint API can still receive the DID it gets elsewhere, internally or at some point given by the buyer"* — correctly identifies **G4** as the dead-field surgery + confirms the mint API surface should stay as-is. The other five gaps are downstream.

**Verification step (pre-Findings):** searched RFC-0957 §Wire Format + `crates/octo-wallet/src/capability/wire.rs` (top of file). The wire is 3 segments: `base64url(macaroon) || "." || base64url(holder_sig) || "." || base64url(discharges_bag)`. **No holder_did.** The wire is already correct. The user's framing targets the egress-side `CapabilityHandle.holder_did` field (F4), not the wire substrate.

**Recommended path:** Two amending RFCs (RFC-0957-A1 stub + RFC-0959-A1) + three new RFCs (RFC-0969 dual-pipeline, RFC-0970 hop auth, RFC-0971 role consolidation) + 1 mission fix.

---

## Problem Statement

The original design intent (recovered from RFC-0957 design goals + RFC-0959 roles and authorities + RFC-0870 system architecture + this session's user statement):

- CipherOcto runs a **mesh of router nodes** that forward inference requests toward the destination node that holds the provider key.
- The destination node is the **mints** — it issues both bearer tokens (for legacy clients) and capability tokens (for wallet-side clients) for the providers it re-sells.
- The destination node holds the **catalog map**: `DID -> holder_pub -> caveats -> rate-limit -> provider`. This is its local knowledge of every holder it has ever minted for.
- When a holder wants to use a legacy client, they opt for the bearer path. When their client is capability-aware, they opt for the capability path. The destination node's catalog must support both.
- When the destination node decides to **re-sell access** in the quota market (RFC-0955 + RFC-0959), the buyer (B) provides its DID to the seller (S) at deal time. S mints a capability token for B *bound to the Ask* (RFC-0959 `AskBinding` caveat, RFC-0957 caveat DSL). S also has a legacy bearer for B's legacy client. S delivers both.
- B and S both store the same `(B_did, holder_pub, caveats, ask_id)` tuple. The wire does not need to carry `holder_did` because both sides already have it. The mint API doesn't need the buyer to supply it at mint time — S's catalog already has it.

Today's spec state:

- ✅ Wire format excludes holder_did (RFC-0957 §Wire Format).
- ✅ Catalog map is implicit (RFC-0957 roles and authorities calls Token Issuer "persisted on `CapabilityToken.holder_did`" but the catalog storage layer is unspecified).
- ✅ Mint API surface: stays as-is by design (user clarification 2026-08-01). The mint receives `holder_did` from the caller at deal time; the parsed `CapabilityToken` keeps the field. The wallet-side parsed struct is the source of truth — the egress-side `CapabilityHandle.holder_did` is the F4 dead field, NOT the mint API parameter.
- ❌ Forwarding behavior at intermediate hops is unspecified for either auth path (see F1, G1).
- ❌ Dual-issuance (bearer + capability for the same holder) is not a spec concept (see F1, G2).
- ❌ Verifier resolver is unspecified — `deserialize_wire` takes `holder_did` as parameter; the "out-of-band resolution" is a TODO comment (see F3, G3).
- ❌ Destination node role binding is implicit (RFC-0870 Router + RFC-0957 Token Issuer + RFC-0959 Asker are listed as separate roles with no explicit "same node" assertion — see F5, G5).
- ❌ Market delivery envelope is un-spec'd (RFC-0959 settles the deal but does not specify the bearer + capability delivery artifact — see F6, G6).

The gap is **a missing coherent design for the destination-node-as-mints + catalog-as-resolver + dual-pipeline authorization + forwarding-hop semantics**. The pieces exist in scattered RFCs; the consolidation never happened.

---

## Research Scope

**Included:**
- Dual-mode authentication: how legacy bearer + capability coexist at the same gateway
- Forwarding hops: what auth envelope they carry; intermediate vs destination behavior
- Mint API surface: stays as-is (mint receives holder_did from caller; parsed `CapabilityToken` keeps the field)
- Catalog/storage: where the destination node stores `(did, holder_pub, caveats, ask_id, ttl)` tuples
- Market delivery: when the seller hands both bearer + capability to the buyer
- Resolver semantics: how the verification side resolves the holder_did at parse time

**Excluded:**
- Provider-key vault (RFC-0009 vault — Provider-Key Handling) — already speced, no changes needed
- The macaroon crypto itself (RFC-0957 §Algorithms) — already speced
- The ask settlement chain (RFC-0959) — already speced; gaps identified are upstream
- ZK capability subclass (RFC-0958) — separate concern, base class gaps are load-bearing
- Cross-provider correlation analysis (RFC-0957-b R9) — already addressed

---

## Findings

### Finding F1: Two pipelines, one wire, no spec for the difference

**What exists:**
- RFC-0917 defines **litellm-mode** vs **any-llm-mode**. This is **provider integration strategy** (reqwest vs PyO3), NOT auth strategy. Both modes expose both HTTP proxy and Python SDK.
- RFC-0903 is the centralized virtual-key management layer. Tokens are `sk-...` style, validated by HTTP proxy auth middleware.
- RFC-0957 §Wire Format defines the capability token wire. RFC-0957 alternative header form: *"Alternative: `Authorization: CipherOcto-Cap <...>` (when bearer coexists)"* — this is the only spec-level hint that both could be on the same request.
- RFC-0949 is for enterprise browser users; separate concern.

**Gap:** RFC-0917 is misread as addressing the dual-mode auth question. It does not. The "dual-mode" name is reused for two different concepts (provider integration strategy vs auth strategy). The actual legacy-bearer + capability dual-pipeline is only implicitly described in RFC-0957 alternative header form.

**Boundary check:** RFC-0917 mentions *"virtual keys (RFC-0903) — HTTP proxy only (Python SDK callers bypass proxy, no virtual key enforcement)"* — this is the auth-path split, but it's about HTTP-vs-Python, not bearer-vs-capability.

**Classification:** **New RFC.** Concept is significant enough to merit its own document. Roughly:

> **RFC-0969: Dual-Pipeline Authorization — Legacy Bearer + Capability**
>
> - §Wire Format: `Authorization: Bearer <sk-...>` OR `X-Capability-Token: <macaroon>` OR `Authorization: CipherOcto-Cap <macaroon>` (when bearer coexists per RFC-0957 alternative header form)
> - Router-side: gateway middleware MUST accept both, choose parse path based on header prefix
> - Destination side: same node mints both; catalog unifies both into `HolderRecord { did, holder_pub, bearer_capsule, capability_token_root, ... }`
> - Backward compat: legacy clients (claude-code, hardcoded agent HTTP, etc.) continue using bearer with no client-side change
> - Forward compatibility: new clients opt into capability by including the wallet-side signer

**Confidence:** HIGH. The wire format is already this; the spec just needs to acknowledge it.

### Finding F2: Forwarding hops — auth envelope is undocumented

**What exists:**
- RFC-0870 defines `ForwardRequest` envelopes with TTL≤3 hops. The envelope carries `request: serialized HTTP request + headers`. Auth headers travel in the envelope transparently.
- RFC-0870 peer trust: *"Do NOT forward `known_peers` from untrusted peers (`PeerTrust::Untrusted`)."* — this is about peer meta, not auth.
- RFC-0870 G1: *"< 100ms p50 for 3-hop propagation."* — performance budget for forwarding.
- RFC-0870 Roles table: Router Node lifecycle includes forwarding but does NOT pin auth responsibilities.

**Gap:** When a `ForwardRequest` arrives at an intermediate router, does that router:
- (a) Verify the inner auth header before forwarding? **Verified NO** — `crates/quota-router-core/src/node/forward.rs` has no auth field; `handler.rs:116 handle_forward_request` deserializes the payload but does not inspect auth headers
- (b) Add/append the destination DID? **Verified NO** — `forward.rs` has no DID field on the envelope
- (c) Re-mint a scoped-down capability for the next hop? **Verified NO** — RFC-0870 TTL only; no per-hop mint
- (d) Strip the inner auth and replace with its own? **Verified NO** — handler does not touch the inner request's auth headers

**The forward-then-verify model assumes the destination is the verifier.** But RFC-0870 forwarding trigger shows that forwarding is selected when local capacity is insufficient — the destination node is the verifier.

**The gap is whether intermediate hops can be trusted with the bearer/capability.** Two design options:

| Option | Threat model | Performance | Verdict |
|---|---|---|---|
| **Transitive trust** — hops forward the inner auth unchanged | Trust all hops (mesh-level adversary) | Fast | Rejected — RFC-0853 overlay cryptography specifies hop-by-hop channel binding |
| **Channel-wrapped re-issuance** — every hop gets a fresh, scope-narrowed capability for the next hop | Trust only the destination | Slower (re-mint per hop) | Proposed (RFC-0957 has no spec for per-hop scope yet) |

**Classification:** **New RFC (or amendment to RFC-0870).** Stricter: this is a new RFC because it changes the wire envelope from RFC-0870. Suggested:

> **RFC-0970: Forwarding-Hop Authorization Envelope**
>
> - Hops-as-untrusted: intermediate router nodes are NOT trusted with the long-lived bearer or capability token
> - Per-hop channel: each hop wraps the inner request in a per-hop capability (TTL ≤ next hop's RTT, scope ≤ model+rate, audience = the next hop's DID)
> - Destination unwrap: destination node unwraps chain, sees original bearer/capability, runs its own verification
> - Cross-hop verifiability: per-hop channel binding via RFC-0853 overlay cryptography (BLAKE3 keyed-hash over hop envelope + next-hop DID)
> - Dependency: requires RFC-0957 capability format (Accepted) + RFC-0853

**Confidence:** MEDIUM. The "transitive trust" rejection is forced by RFC-0853. The per-hop envelope is a design choice; alternative is "destination-only auth + opaque envelope" (use the channel layer for hop-by-hop encryption, don't re-issue capabilities).

### Finding F3: holder_did resolution — "out-of-band" is a TODO, not a design

**What exists:**
- RFC-0957 roles and authorities: *"Token Issuer: `DID` (per RFC-0009 §Identity Key Format); persisted on `CapabilityToken.holder_did`."* — the catalog exists as a concept but is not a spec module.
- RFC-0957 verify context: `VerifyContext::root_secret_lookup: Box<dyn Fn(&[u8; 32]) -> Option<[u8; 32]>>` — the root secret catalog is a function pointer; no schema.
- RFC-0957 verify context: `VerifyContext { discharges, channel_providers, clock, root_secret_lookup }` — the verify context already has 4 slots; one of them COULD be the DID resolver, but the spec doesn't name it.
- `crates/octo-wallet/src/capability/mod.rs:119`: `mint(root_secret: &[u8; 32], holder: &IdentityKey, holder_did: impl Into<String>, ...)` — holder_did is a parameter.
- `crates/octo-wallet/src/capability/wire.rs:84-86`: *"Holder DID + public key are NOT in the wire format — caller passes them as parameters (resolved out-of-band from a DID registry)."*

**Gap:** The "DID registry" mentioned in the wire spec comment doesn't exist. The closest is the `AudienceId` from RFC-0009 identity, which is an opaque string (`crates/octo-wallet/src/identity.rs::AudienceId::from_str` accepts any non-empty string).

**What the destination node needs to resolve at verify time:**
- Given `cap_root_hash` (from the wire) → find the local `HolderRecord { did, holder_pub, caveats, ask_id, ttl, scope }`
- This is the "catalog map" the user references

**Design options:**

| Option | Storage | Lookup | Verdict |
|---|---|---|---|
| **(a) In-memory `HashMap<CapRootHash, HolderRecord>`** | Process-local | O(1) | No — lost on restart, no federation |
| **(b) Stoolap table `holder_registry(cap_root_hash PK, did, holder_pub, caveats JSON, ask_id, mint_at_unix, ttl_unix)`** | Persistent | O(log n) index | Adopt — reuses stoolap (RFC-0862) |
| **(c) Local file `~/.config/cipherocto/holders/<cap_root_hash>.holder`** | Persistent | O(1) file open | Rejected — same shape as provider-key vault but security-sensitive + read-on-every-request |
| **(d) Skip storage — derive holder_pub from holder_sig in the wire** | None | Wire-only | Rejected — defeats the "out-of-band" design |

**The (b) option is the only one that fits the cipherocto substrate:**
- Stoolap is the persistence layer (RFC-0862 Accepted)
- Sync between destinations is automatic via RFC-0862 gossip
- Index is `cap_root_hash` (deterministic, BLAKE3-derived, 32 bytes — perfect primary key)
- Schema is straightforward: `did, holder_pub, caveats, ask_id, mint_at, ttl`

**Classification:** **In-place amendment to RFC-0957.** Add a new "Catalog Storage" section + new `HolderRegistry` trait + new `StoolapHolderRegistry` impl. The existing `VerifyContext` extended with `holder_registry: Box<dyn HolderRegistry>`. The mint side also writes to the registry.

**Confidence:** HIGH. The substrate is there; the spec just needs to bind it.

### Finding F4: `CapabilityHandle.holder_did` is dead — remove the field

**What exists:**
- `crates/quota-router-core/src/egress.rs:191-200` defines:
  ```rust
  pub struct CapabilityHandle {
      pub cap_root_hash: [u8; 32],
      pub holder_did: String,    // ← always empty
  }
  ```
- `egress.rs:299`: `holder_did: String::new(), // populated by the verifier layer` — the comment is aspirational; no verifier layer exists in the workspace.
- `egress.rs:286`: `holder_did: String::new()` — the no-capability-token path also returns empty.
- `egress.rs:611`: `assert_eq!(handle.holder_did, "")` — test asserts the field is empty.
- `egress.rs:188-189`: *"`None` for `cap_root_hash` and `holder_did` means no capability was attached"* — the doc comment treats the empty field as a sentinel.

**Gap:** The field is a structural dead column. It is initialized to `String::new()` at every code path that constructs `CapabilityHandle`. The "verifier layer" the comment references does not exist in the workspace. Cross-referencing `proxy.rs::extract_capability_token` (returns only `Option<String>`, never parses the wire), the field has no producer.

**The mint API is NOT in scope for this fix.** The user clarified: *"the mint API can still receive the DID it gets elsewhere, internally or at some point given by the buyer."* The mint API stays as-is. The parsed `CapabilityToken` wallet-side (`crates/octo-wallet/src/capability/mod.rs:58`) keeps its `holder_did: String` field because the wallet has the DID at mint time and the catalog provides it at parse time.

**Design decision:** Drop `holder_did` from `CapabilityHandle`. The struct becomes:

```rust
pub struct CapabilityHandle {
    pub cap_root_hash: [u8; 32],
}
```

The egress-side handle is now a thin wrapper around the cap root hash. Any downstream consumer that needs the holder identity obtains it from the wallet-side parsed `CapabilityToken` (per F3 design) or from the wire's parse path (`deserialize_wire(s, holder_did, holder_pub)`) at the wallet boundary — not from the egress-side handle. The F4 fix is independent of F3; consumers can adopt the new path incrementally.

**Side effects to clean up:**
- `egress.rs:188-189` doc comment updated: no more "None for holder_did" sentinel language
- `egress.rs:286, 299` field-initialization removed
- `egress.rs:611` test assertion removed
- Any consumer of `handle.holder_did` (grep `crates/quota-router-core/`) updated to obtain the DID from the wire-parse path or the catalog

**Classification:** Mission-scale fix. Lives in `missions/claimed/0957-b-provider-boundary-exercise-path.md` R9-4 closure. No new RFC needed — the field is implementation, not spec.

**Confidence:** HIGH. The user has the call site. Surgical removal.

### Finding F5: Destination node is the mints — no role-binding

**What exists:**
- RFC-0870 roles and authorities: Router Node has lifecycle "(Designated, Elected, Active, ...)" — does NOT mention minting capability.
- RFC-0957 roles and authorities: Token Issuer is the *holder* (RFC-0009 identity). The seller is unspecified as a role.
- RFC-0959 roles and authorities: Asker = the node that publishes the Ask. Router = the node that verifies the capability at consumption time. RFC-0959 role catalog lists Asker and Router as separate roles.

**Gap:** The destination node (RFC-0870 "Router") is one role. The capability token issuer (RFC-0957 "Token Issuer") is another role. RFC-0959 "Asker" is a third. **Verified separately** in RFC-0959 role catalog — Asker and Router are listed as distinct roles with no explicit "same node" assertion. The user's framing assumes the destination node holds all three roles; the RFCs do not bind them explicitly. This is the gap.

**What the spec needs:**
- Designate the destination node as holding the union of: RouterNode + TokenIssuer + Asker.
- Specify that the destination node's `HolderRegistry` (F3) is the source of truth for all holders it has minted for.
- Specify that when the destination node re-sells access (RFC-0955 marketplace), it adopts the role of "Seller" with its own DID + escrow + reputation.
- Specify that the destination node's forward-departure behavior at egress (RFC-0957-b) is the same node function regardless of whether the inbound was bearer or capability.

**Classification:** **New RFC or amendment to RFC-0870.** The role-binding is fundamental enough to warrant a dedicated section. Suggested:

> **RFC-0971: Destination-Node as Mint-Holder — Role Consolidation**
>
> - Roles: Router + TokenIssuer + Asker are the same node. Spec-merge the three.
> - HolderRegistry: one per node; synced via RFC-0862 across peer set
> - Market integration: when the node re-sells, it adopts Seller role + dual-pipeline (bearer + capability)
> - Capability mint on inbound: verify at destination; mint downstream caveats for the next hop (per F2)

**Confidence:** MEDIUM. The role-binding is implicit in the architecture; explicit naming is a gap only because no spec has crystallized it.

### Finding F6: Market delivery — both bearer + capability at deal time

**What exists:**
- RFC-0959 defines Ask signed by Asker, SettlementEvent signed by Router, SettlementReceipt signed by Router. No spec for the buyer's authorization token.
- RFC-0955 defines compute markets + proof markets + storage markets. No spec for the access token delivery.
- RFC-0900 — referenced as Draft.

**Gap:** The user describes the deal flow:

1. Buyer (B) registers with Seller (S) — gives B's DID to S.
2. S publishes an Ask (RFC-0959).
3. B selects the Ask; deal settles (RFC-0959 SettlementEvent).
4. S delivers the authorization to B — both bearer (for legacy clients) + capability token (for wallet-side clients), bound to the ask_id.
5. B stores the capability token; uses it for all subsequent requests.

**Note:** Step 4 is the un-spec'd surface. Steps 1-3 + 5 are covered by RFC-0959 + RFC-0955.

**What's missing in spec:** Step 4. The spec says "Asker signs the Ask; Router signs the receipt." It does not say "Seller delivers the bearer+capability."

**This is a gap because the dual-mode workflow requires both tokens to be delivered atomically. The legacy bearer is for legacy clients (no signing, no keypair on the client side). The capability token is for wallet-side clients. The buyer may switch between them at will; the seller must support both.**

**Design options:**

| Option | Where bearer comes from | Where capability comes from | Atomic? |
|---|---|---|---|
| **(a) Same catalog entry holds both** | `HolderRegistry` adds `bearer_capsule: EncryptedBlob` alongside `capability_root` | Same table | Yes — single write |
| **(b) Separate bearer system (RFC-0903) + capability (RFC-0957)** | Eager RFC-0903 virtual key | Eager RFC-0957 mint | No — two distinct deals |
| **(c) Issuer picks at delivery time** | Eager if buyer opted in | Eager if buyer opted in | Yes — at delivery |

**The user's framing implies (a)**: the catalog entry holds both. This is the cleanest because the HolderRegistry already exists per F3.

**Classification:** **In-place amendment to RFC-0959** (Market Delivery Envelope) + new section in RFC-0957-A1 (holder registry schema includes bearer capsule).

**Confidence:** HIGH. The R9 work in 0957-b already did the egress side; the delivery side is the symmetric upstream.

---

## Recommendations

### Summary table

| Gap | Action | Document | Effort |
|---|---|---|---|
| G1 / F1 | New RFC | **RFC-0969 (dual-pipeline authorization)** | 1 session |
| G2 / F2 | New RFC | **RFC-0970 (forwarding-hop auth envelope)** | 1-2 sessions |
| G3 / F3 | RFC-0957 amendment | **RFC-0957-A1** (catalog storage + holder registry) | 1 session |
| G4 / F4 | Mission-scale fix | **0957-b R9-4 closure** (drop `CapabilityHandle.holder_did` field) | trivial |
| G5 / F5 | New RFC or RFC-0870 amendment | **RFC-0971 (destination-node role consolidation)** | 1-2 sessions (after prerequisites) |
| G6 / F6 | RFC-0959 amendment + RFC-0957-A1 | **RFC-0959-A1** (market delivery envelope) | 1 session |
| Verification (was F7) | None — subsumed by F4 | n/a | n/a |

### Per-document recommendations

**0957-b R9-4 closure (mission fix, no RFC): Drop `CapabilityHandle.holder_did`**
- File: `crates/quota-router-core/src/egress.rs`
- Remove `holder_did: String` field from `CapabilityHandle` (holder field)
- Remove `String::new()` initializers in constructors
- Remove `assert_eq!(handle.holder_did, "")` in tests
- Update doc comment in holder field (no more "None for holder_did" sentinel)
- Side effect: any consumer of `handle.holder_did` (grep across `crates/quota-router-core/`) updated to obtain the DID from the wire-parse path (`deserialize_wire(s, holder_did, holder_pub)`) — wallet-side, not egress-side
- Mint API (`crates/octo-wallet/src/capability/mod.rs:119`) STAYS AS-IS — `holder_did` parameter preserved

**RFC-0957-A1 (in-place amendment): Holder Registry + Catalog Storage**
- Section: §Algorithms — `mint()` signature STAYS (`mint(root_secret, holder, holder_did, caveats, catalog)`); no parameter changes
- Section: §Roles and Authorities — Token Issuer row updated to mention `HolderRegistry` (RFC-0862-backed stoolap table)
- Section: §Data Structures — `CapabilityToken.holder_did` populated at parse time from the wallet's local catalog
- Section: Mandates (reference impl) — `StoolapHolderRegistry` reference impl
- Section: §Security Considerations — Adversary A5 row updated to reflect that the registry's stoolap sync is the channel-binding
- Implementation: `crates/octo-wallet/src/capability/{registry,wire}.rs` + new `crates/octo-wallet/src/capability/holder_registry.rs`
- Missions: extend `0957-a` (registry) + new `0957-c` (registry impl + wire rewrite if needed)

**RFC-0959-A1 (in-place amendment): Market Delivery Envelope**
- Section: §Roles and Authorities — Asker row updated to mention Seller = the same node
- Section: §Lifecycle Requirements — new event `DealSettled { buyer_did, seller_did, ask_id, bearer_capsule_hash, capability_root_hash, ... }`
- Section: §Algorithms — `deliver_at_settlement(buyer_did, seller_did, ask_id) -> (BearerCapsule, CapabilityToken)` that pulls both from the HolderRegistry
- Section: §Compatibility — references RFC-0957-A1
- Implementation: `crates/octo-wallet/src/capability/{market,delivery}.rs` (or extension of existing 0959 crate)

**RFC-0969 (new): Dual-Pipeline Authorization**
- Section: §Wire Format — both legacy bearer and capability on the same request envelope
- Section: §Roles and Authorities — Gateway Authenticator role (new)
- Section: §Algorithms — header-based router: `Authorization: Bearer ...` → RFC-0903 path; `X-Capability-Token: ...` → RFC-0957 path; `Authorization: CipherOcto-Cap ...` → RFC-0957 alt path
- Section: §Roles and Authorities — Buyer + Legacy Client + Capability Client
- Section: §Lifecycle Requirements — BearerLifecycle (Issue → Active → Revoked) parallel to CapabilityToken state machine
- Section: §Adversary Analysis — on the dual-pipeline path

**RFC-0970 (new): Forwarding-Hop Authorization Envelope**
- Section: §Wire Format — outer envelope wraps inner request; per-hop channel
- Section: §Roles and Authorities — Intermediate Router (untrusted)
- Section: §Algorithms — per-hop re-mint: `mutate_for_hop(capability, next_hop_did, ttl = RTT)`; destination unwraps
- Section: §Adversary Analysis — full analysis assuming malicious hop
- Section: §Compatibility — RFC-0870 ForwardRequest envelope extended

**RFC-0971 (new): Destination-Node Role Consolidation**
- Section: §Roles and Authorities — Router + TokenIssuer + Asker + (optionally) ReputationAnchor are the same node
- Section: §Lifecycle Requirements — combined state machine
- Section: §Specification (storage) — single HolderRegistry on the destination node
- Section: §Compatibility (backward) — RFC-0870 Router role unchanged; new role binding is meta

### Sequencing

```
0957-b R9-4 closure (drop CapabilityHandle.holder_did field)  — trivial, immediate
   ↓
RFC-0957-A1 (holder registry + catalog)   — foundation
   ↓
RFC-0959-A1 (market delivery)             — uses 0957-A1 registry
   ↓
RFC-0969 (dual-pipeline)                  — uses 0957 + 0959
   ↓
RFC-0970 (forwarding hops)                — uses 0957 + 0969
   ↓
RFC-0971 (role consolidation)             — meta, summarizes all four
```

1 mission fix + 5 RFCs (2 amendments + 3 new), 4-5 sessions of work. Commits free, push + remote writes need explicit user instruction (per repo convention).

### Out of scope (deferred)

- ZK capability subclass (RFC-0958) — already separate; the dual-pipeline authority DOES extend to ZK, but that's its own scoping exercise
- Provider-key vault (RFC-0009 vault — Provider-Key Handling) — already speced
- Cross-provider correlation analysis (RFC-0957-b R9) — already addressed
- Wallet-side derivation key binding (RFC-0009 §Capability Keys) — already speced

---

## Next Steps

### Immediate (next session)

1. **Close 0957-b R9-4** — drop `holder_did: String` from `CapabilityHandle` in `crates/quota-router-core/src/egress.rs`. Update holder-field doc, constructors initializers, tests assertion. Side-effect: any consumer of `handle.holder_did` updated to parse from wire (`deserialize_wire(s, holder_did, holder_pub)`). Mint API unchanged.
2. **Update `missions/claimed/0957-b-provider-boundary-exercise-path.md`** R9-4 carryover table — mark as CLOSED (mission-scale fix, not RFC).
3. **Create RFC-0957-A1** in `rfcs/draft/economics/0957-a1-holder-registry.md`. Cover F3 (holder registry + catalog). Mint API signature stays as-is per user clarification.

### Verification gate

Before any RFC reaches Accepted:

- [ ] `CapabilityHandle.holder_did` field removed; egress.rs builds clean
- [ ] Mint API signature unchanged (`mint(root_secret, holder, holder_did, caveats, catalog)`)
- [ ] `HolderRegistry` trait implemented with `StoolapHolderRegistry`
- [ ] Market delivery sends both bearer + capability
- [ ] Forwarding hops unwrap per-hop channel
- [ ] RFC-0957-b R9-4 carryover entry marked CLOSED

### Medium-term (1-2 sessions)

4. **Create RFC-0959-A1** (Market Delivery Envelope). This is the upstream of the dual-mode authorization work.
5. **Create RFC-0969** (Dual-Pipeline Authorization). Canonicalize the legacy bearer + capability coexistence.

### Long-term (3-5 sessions)

6. **Create RFC-0970** (Forwarding-Hop Authorization Envelope). Address the multi-hop auth traversal.
7. **Create RFC-0971** (Destination-Node Role Consolidation). Meta RFC summarizing the others.

### Cross-RFC consistency

- RFC-0957-A1 Requires: RFC-0957, RFC-0009, RFC-0862
- RFC-0959-A1 Requires: RFC-0957-A1, RFC-0959, RFC-0009
- RFC-0969 Requires: RFC-0957-A1, RFC-0903
- RFC-0969 Optional: RFC-0949 — enterprise SSO path
- RFC-0970 Requires: RFC-0957-A1, RFC-0870, RFC-0853
- RFC-0971 Requires: RFC-0957-A1, RFC-0959-A1, RFC-0969, RFC-0970, RFC-0870

DAG check: no cycles. RFC-0971 depends on all four; that is the final.

---

## Cross-Reference Map

| Spec / Mission | In scope | Gap addressed |
|---|---|---|
| `rfcs/accepted/economics/0957-capability-token-format.md` | Capability wire format | F1 (dual pipeline), F3 (catalog); F4 only as footnote (mint API unchanged) |
| `rfcs/accepted/economics/0959-ask-settlement-chain.md` | Settlement chain | F6 (market delivery) |
| `rfcs/accepted/economics/0917-dual-mode-query-router.md` | Dual-mode provider integration | F1 (overlap with new RFC-0969 — different concept) |
| `rfcs/accepted/economics/0903-B1-schema-amendments.md` | Virtual keys | F1 (legacy bearer) |
| `rfcs/accepted/networking/0870-distributed-quota-router-network.md` | Forwarding | F2 (hop auth), F5 (destination role) |
| `rfcs/accepted/process/0009-identity-management.md` | Vault + identity | F3 (catalog drops vault dependency) |
| `rfcs/accepted/networking/0862-stoolap-data-sync.md` | Stoolap sync | F3 (catalog uses stoolap) |
| `rfcs/accepted/networking/0853-overlay-cryptography.md` | Per-hop crypto | F2 (per-hop channel) |
| `rfcs/accepted/economics/0955-model-liquidity-layer.md` | Marketplace | F6 (delivery) |
| `missions/claimed/0957-b-provider-boundary-exercise-path.md` | R9 audit | F4 (closure of R9-4) |
| `missions/claimed/0957-a-capability-token-macaroon.md` | Mint + verify | none (mint API unchanged; F4 is in 0957-b egress-only) |
| `crates/octo-wallet/src/capability/{mod,wire,macaroon}.rs` | Code surface | F3 (registry); mint API unchanged |
| `crates/quota-router-core/src/egress.rs` | Egress-side | F4 (drop `CapabilityHandle.holder_did` field) |

---

## Conclusion

The dual-mode workflow is **architecturally sound** but **spec-fragmented**. One trivial mission fix + five RFCs (two amendments + three new) close the gap. The user's framing — drop the empty `holder_did` field from the deserialized `CapabilityHandle` struct, keep the mint API signature as-is — targets **G4** exactly: surgical removal of a struct field that has no producer. The dual-pipeline (G1), forwarding-hop auth (G2), destination-node role consolidation (G5), and market delivery envelope (G6) are independent gaps that need spec; the catalog (G3) is the foundation for all of them.

**Viable → Use Case → 1 mission fix + 5 RFCs.** The gulp is large but the work is mechanical: most of the substrate is already in the codebase; the spec just needs to bind it.

**Status: Viable → Use Case → RFC cluster (2 amendments + 3 new) + 1 mission fix.** See `docs/use-cases/dual-mode-authorization-workflow.md` (next work) for the narrative layer.
