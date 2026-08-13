# End-to-End Scenarios: From Local Prompt to Network-Routed Inference

**Status:** Draft (2026-08-13)
**Audience:** New contributors. Read this after `ARCHITECTURE.md` to understand how the pieces fit together through the lens of a single request.
**Scope:** Buyer-seller flow across the local proxy, the marketplace, the mesh, and the seller node. Failures and adversarial cases included.

> This document is **integrator glue**. Each scenario cross-links the deeper docs that own the specifics. Use it as a tour, then drill into the linked RFCs and architecture specs for protocol-level detail.

## Glossary

| Term                   | Meaning                                                                                                           | Where                                                                           |
| ---------------------- | ----------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------- |
| **Hardened client**    | Production CipherOcto client (CLI / Python SDK / HTTP). Talks to local proxy on `localhost`.                      | `../quota-router-python-sdk.md`, `architecture/quota-router-architecture.md §4` |
| **Local proxy**        | `quota-router-core` HTTP server running on the buyer's machine. First hop.                                        | `architecture/quota-router-architecture.md`                                     |
| **Provider**           | LLM API (OpenAI, Anthropic, etc.). Reached by API key configured on the buyer or seller node.                     | `architecture/quota-router-architecture.md §5`                                  |
| **`dispatch_map`**     | Map of `model_name → DispatchInfo { provider, api_base, rpm }`. Decides which provider serves which model.        | `architecture/quota-router-architecture.md §9.2`                                |
| **CapabilityToken V2** | Bearer token signed by buyer; specifies model, max price, expiry, caveats. Outer envelope `CapabilityBundleV2`.   | `use-cases/dual-mode-authorization-workflow.md`, RFC-0957                       |
| **CapabilityBundleV2** | RFC-0957 stable outer envelope. Inner carries caveats. V2 is cutover target; V1 still accepted during migration.  | RFC-0957                                                                        |
| **Marketplace node**   | Discovery + reputation registry. Returns ranked offers for a requested model.                                     | `use-cases/ai-quota-marketplace.md`, RFC-0969                                   |
| **Seller node**        | A `quota-router-core` operator who exposes their provider pool to the network. Earns revenue.                     | RFC-0969, `architecture/octo-network-architecture.md`                           |
| **Wallet node**        | Mints + verifies capability tokens. Holds the buyer's signing key (HSM-mandated for production).                  | `use-cases/dual-mode-authorization-workflow.md`, mission `0009-a-hsm-routing`   |
| **NodeEnvelope**       | Unified mesh wire envelope (`PayloadKindId`, `Authorization`, `SignerDid`).                                       | `architecture/octo-network-architecture.md §3.3`, RFC-0871 §Data Structures     |
| **`PayloadKindId`**    | UUID discriminator inside `NodeEnvelope` identifying the inner payload type (one of 7 per RFC-0870).              | mission `0870-b-envelope-adoption`                                              |
| **HopSignature**       | Per-hop Ed25519 signature over the canonical chain hash.                                                          | RFC-0871 §Data Structures                                                       |
| **Router node**        | Mesh relay. Forwards envelopes hop-by-hop, charges a relay fee.                                                   | `architecture/octo-network-architecture.md §9-11`                               |
| **ORR**                | Onion Relay Routing. Router hops see only next hop, not final destination. Privacy by construction.               | `architecture/octo-network-architecture.md §11`                                 |
| **PoRelay**            | Proof-of-Relay. Trust registry scoring router hops by stake weight + history.                                     | `architecture/octo-network-architecture.md §13`                                 |
| **Balance**            | In-memory per-key monetary counter on the proxy. Decremented per request, checked pre-dispatch. Distinct from the | `crates/quota-router-core/src/balance.rs`                                       |
|                        | storage-layer `budget_limit` field (which is a top-of-funnel cap on key creation).                                |                                                                                 |
| **TokenBucket**        | Per-key rate-limiter on the proxy. Returns `bool` from `try_consume`. Refilled at configured RPM.                 | `crates/quota-router-core/src/key_rate_limiter.rs`                              |
| **Escrow**             | Buyer pre-funds a payment vault tied to the capability. Released on successful response, refunded on failure.     | RFC-0969, marketplace facade                                                    |
| **Settlement**         | Post-completion: escrow releases to seller node + router hops, reputation scores update.                          | `use-cases/reputation-persistence.md`                                           |
| **seen-set**           | Seller-side Bloom filter / LRU cache of recently processed `envelope_id` values. Replay defense at the seller.    | RFC-0871 §TV3                                                                   |

> Throughout this doc, **"envelope"** is shorthand for `NodeEnvelope` (the full proper noun) outside formal type references. The abbreviated form is fine in prose; the formal name appears in code snippets and capability-witness fields.

## Cross-references

- Top-level overview: [`ARCHITECTURE.md §Data Flow: End-to-End Inference`](../ARCHITECTURE.md#data-flow-end-to-end-inference) (the consensus view; this doc adds the buyer-seller perspective)
- Local proxy request paths: [`architecture/quota-router-architecture.md §4`](../architecture/quota-router-architecture.md#4-request-flow)
- Mesh substrate: [`architecture/octo-network-architecture.md`](../architecture/octo-network-architecture.md) (DOT, GDP, MON, DRS, ORR, PCE, PoRelay)
- Marketplace narrative: [`use-cases/ai-quota-marketplace.md`](../use-cases/ai-quota-marketplace.md)
- Dual-mode auth flow: [`use-cases/dual-mode-authorization-workflow.md`](../use-cases/dual-mode-authorization-workflow.md)
- Existing e2e test plans (DOT pipeline, 2026-06): [`e2e/2026-06-16-e2e-test-plan.md`](../e2e/2026-06-16-e2e-test-plan.md)

---

> Shorthand in the mermaid diagrams below: **RR** = `ReputationRegistry` (records `success` / `dispute` / `replay` outcomes; reputation scores for sellers, routers, buyers); **E** = `EscrowLedger` (the per-capability payment vault from Scenario 7); **ST** = PoRelay `Registry` (router/hop trust registry from Scenario 14 — exposes `get_stake(did)`); **D** = `DisputeRegistry` (escrow + slashing coordinator from Scenario 13). "Wallet node" / "Marketplace node" / "Router node" / "Seller node" remain the glossary terms above.

## Scenario index

> **Note on numbering.** The L1–L8 labels below are **narrative phases** of the buyer-seller journey (Local → Marketplace → Deal → Mesh → Seller → Settlement → Adversarial), NOT the cryptographic-architecture layers defined in `CLAUDE.md §Architectural Principles` (Layer A crypto substrate / B identity + transport / C specialized nodes / D transport adapters / E user extensions). Both schemes are correct in their respective contexts; they are kept distinct here to avoid collision.

| #   | Phase                 | Title                                                           |
| --- | --------------------- | --------------------------------------------------------------- |
| 1   | P1 Local              | Hello world — single provider, no network                       |
| 2   | P1 Local              | Multi-provider dispatch via `dispatch_map`                      |
| 3   | P2 Local failure      | Provider 500 surfaces as 502 (pinned)                           |
| 4   | P2 Local failure      | Budget exhausted → 402 (pinned)                                 |
| 5   | P2 Local failure      | Rate limit exceeded → 429                                       |
| 6   | P3 Marketplace        | Discovery — model not local, query marketplace                  |
| 7   | P4 Deal               | Pick offer, mint CapabilityToken V2, fund escrow                |
| 8   | P5 Mesh               | NodeEnvelope + 2-hop mesh + HopSignature chain                  |
| 9   | P6 Seller             | Verify capability (ZK + macaroon caveats), reputation, dispatch |
| 10  | P7 Streaming + settle | SSE stream back through mesh, escrow release, reputation update |
| 11  | P8 Adversarial        | Replay attack — stale envelope + replayed HopSignature          |
| 12  | P8 Adversarial        | Seller offline mid-stream — partial response + 502 + refund     |
| 13  | P8 Adversarial        | Capability fails server-side — dispute + slash seller stake     |
| 14  | P8 Adversarial        | Sybil seller — fake reputation caught via stake + history       |

---

## Scenario 1 — Hello world (single provider, no network)

The simplest path: a developer on a fresh laptop runs the hardened client (CLI), asks a question, gets an answer from a provider whose API key is configured locally. No marketplace, no mesh, no seller.

```mermaid
sequenceDiagram
    participant U as Developer
    participant C as Hardened client (CLI)
    participant P as Local proxy (quota-router-core)
    participant OAI as OpenAI API

    U->>C: octo prompt "What's the capital of France?"
    C->>P: POST /v1/chat/completions<br/>Authorization: Bearer <api_key>
    P->>P: Validate API key + check balance<br/>(TokenBucket::try_consume(1) + Balance::check(1))
    P->>P: dispatch_map.get("gpt-4o-mini")
    P->>OAI: POST https://api.openai.com/v1/chat/completions<br/>Authorization: Bearer <openai_key>
    OAI-->>P: 200 OK + SSE stream
    P->>P: Append [DONE] + record spend in Balance
    P-->>C: SSE stream forwarded
    C-->>U: "Paris."
```

**Step-by-step:**

1. The hardened client (CLI / SDK) is configured with `base_url = http://localhost:8080` (the local proxy) and a buyer API key.
2. The local proxy authenticates the request: validates the API key against the hot-tier (LRU) key cache, checks the per-key `Balance` field, and applies the per-key token-bucket rate limiter (RFC-0933).
3. The proxy looks up `dispatch_map["gpt-4o-mini"]` to find the provider configuration (`api_base`, `rpm`, etc.). For local-only mode the dispatch map points directly to OpenAI.
4. The proxy forwards the request to OpenAI, streams the SSE response back through the proxy, appends the proxy-owned `[DONE]` terminator, and decrements the buyer's balance.
5. The client renders the response.

**Invariants pinned:**

- Auth: any of {API key, bearer token, capability token} accepted (RFC-0917 §Rust Feature Gates — interface always available, mode controls provider strategy not auth).
- Budget check: `Balance::check(1)` runs before provider call. Insufficient balance → 402.
- Rate limit: per-key `TokenBucket`. Excess → 429 with `Retry-After`.
- Streaming: SSE `data:` lines forwarded verbatim + appended `[DONE]`.

**Failure exits (covered in later scenarios):**

- OpenAI returns 500 → see Scenario 3.
- Balance exhausted → see Scenario 4.
- Rate limit exceeded → see Scenario 5.

---

## Scenario 2 — Multi-provider dispatch via `dispatch_map`

Same prompt, but the developer wants a model they don't have a direct key for. Their `dispatch_map` redirects the request through a different provider.

```mermaid
sequenceDiagram
    participant C as Hardened client
    participant P as Local proxy
    participant D as dispatch_map
    participant ANT as Anthropic API

    C->>P: POST /v1/chat/completions<br/>model="claude-3-5-sonnet"
    P->>P: Validate API key + check balance<br/>(TokenBucket::try_consume(1) + Balance::check(1) — see Scenario 1)
    P->>D: get("claude-3-5-sonnet")
    D-->>P: DispatchInfo {<br/>  provider="anthropic",<br/>  api_base="https://api.anthropic.com",<br/>  rpm=60<br/>}
    P->>ANT: POST /v1/messages (Anthropic format)
    ANT-->>P: 200 OK + SSE
    P->>P: Record spend
    P-->>C: SSE forwarded
```

**Step-by-step:**

1. Buyer requests a model. Proxy resolves the model name through `dispatch_map` (canonical Unicode NFC normalized per RFC-0909 §Design Goals).
2. The matched `DispatchInfo` may specify a different API base, different auth scheme, and different request/response codecs than OpenAI. The provider abstraction (`HttpProvider` or `PyBridgeProvider`) handles the conversion.
3. The proxy forwards the request to the configured provider. Spend is recorded under the buyer's balance.
4. If no `DispatchInfo` matches and the map is non-empty, the proxy returns **503 SERVICE_UNAVAILABLE** with body `"No dispatch entry for model 'X' — provider pool does not serve this model"` (see consolidated 503 table below, row `dispatch-miss`). If the map is empty, the request falls through to the provider-default API base. This asymmetric guard is pinned by `e2e_wiremock_faults::test_dispatch_map_no_match_returns_503`.

**Why this matters:** A single proxy instance can serve requests to multiple providers simultaneously without the client knowing which provider will handle each model. The dispatch map is the routing policy.

---

## Scenario 3 — Provider 500 surfaces as 502 BAD_GATEWAY

OpenAI returns a transient 500. The proxy wraps it. The client retries.

```mermaid
sequenceDiagram
    participant C as Hardened client
    participant P as Local proxy
    participant OAI as OpenAI API

    C->>P: POST /v1/chat/completions
    P->>OAI: POST /chat/completions
    OAI-->>P: 500 Internal Server Error
    P->>P: Log upstream error<br/>Wrap as 502 BAD_GATEWAY
    P-->>C: 502 BAD_GATEWAY<br/>{ "error": "upstream returned 500" }
    C->>C: Retry with backoff<br/>(3 attempts, exp jitter)
```

**Step-by-step:**

1. OpenAI's `/chat/completions` endpoint returns 500 (transient internal error, not a client fault).
2. The proxy's `handle_request_litellm` Err arm maps the upstream 500 → 502 BAD_GATEWAY. The same mapping applies to `handle_streaming` and `handle_embedding_request`. Per RFC-0933 §5. Error Response.
3. The proxy does not retry on the client's behalf. The client is responsible for backoff.
4. If the proxy has a fallback provider configured (e.g. Anthropic as backup), it tries the fallback. The fallback contract is pinned by `proxy.rs::test_post_dispatch_5xx_triggers_fallback`. (Scenario 1's happy path used no fallback.)

**Status-code semantics (pinned):**

- **500 INTERNAL_SERVER_ERROR** = proxy-internal bug. Should not normally occur.
- **502 BAD_GATEWAY** = upstream (provider) fault. Wraps upstream 500.
- **503 SERVICE_UNAVAILABLE** = no provider able to serve the model. Multiple sub-conditions (see consolidated 503 table below).
- **504 GATEWAY_TIMEOUT** = reserved; not currently emitted by the proxy. `classify_http_error` maps incoming 504 responses from upstream to `RouterError::Timeout`, but the proxy itself does not synthesize 504 (streaming-buffer-overflow is not yet implemented).

**Consolidated 503 sub-conditions (verified against `proxy.rs`):**

| Sub-condition                            | Trigger                                                                      | Body                                                                          | Where                                                                                                            |
| ---------------------------------------- | ---------------------------------------------------------------------------- | ----------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| dispatch-miss (asymmetric guard)         | `dispatch_map` non-empty, no entry for requested model                       | `"No dispatch entry for model 'X' — provider pool does not serve this model"` | `proxy.rs::dispatch_map_lookup_503_arm`; pinned by `e2e_wiremock_faults::test_dispatch_map_no_match_returns_503` |
| dispatch-fall-through (map empty)        | `dispatch_map` empty, request falls through to provider's default API base   | falls through (NO 503 emitted here — see Scenario 2)                          | (asymmetric guard intentionally skips this branch)                                                               |
| model-unhealthy (no fallback)            | Dispatch map has the model but health check marks it unhealthy, no fallbacks | `"Model unhealthy"`                                                           | `proxy.rs::health_check_503_arm_no_fallback`                                                                     |
| model-unhealthy (fallback exhausted)     | Primary unhealthy AND all configured fallback models failed                  | `"Model unhealthy and all fallback models failed"`                            | `proxy.rs::health_check_503_arm_fallback_exhausted`                                                              |
| model-unhealthy (no fallback configured) | Primary unhealthy AND no fallback configured                                 | `"Model unhealthy and no fallback models configured"`                         | `proxy.rs::health_check_503_arm_no_fallback_configured`                                                          |
| marketplace-empty                        | Marketplace returns zero offers for the requested model                      | `"no marketplace offers for model X"`                                         | Scenario 6                                                                                                       |
| stoolap-probe-unreachable (test fixture) | (Test-only) Stoolap DB path is unreachable during a probe                    | 503 emitted by the probe handler                                              | `proxy.rs::probe_handler_503_arm` (test assertion)                                                               |

**Why this matters:** Clients distinguish 5xx by source: 500 = bug, 502 = upstream, 503 = no provider. Each triggers a different recovery strategy (bug → report, upstream → retry/backoff, no-provider → reconfigure).

**Test pinned:** `e2e_wiremock_faults::test_upstream_500_returns_502` (commit `7df92475`, mission `proxy-strong-scenarios-phase2`).

---

## Scenario 4 — Budget exhausted returns 402 PAYMENT_REQUIRED

The buyer's per-key `Balance` is zero (or below the request cost). The proxy refuses the request before reaching the provider.

```mermaid
sequenceDiagram
    participant C as Hardened client
    participant P as Local proxy
    participant ST as Stoolap (Balance store)

    C->>P: POST /v1/chat/completions
    P->>P: Validate API key
    P->>ST: SELECT balance_cents FROM balances WHERE api_key = ?
    ST-->>P: 0
    P->>P: Balance::check(1) → false (0 < 1)
    P-->>C: 402 PAYMENT_REQUIRED<br/>{ "error": "balance exhausted" }
```

**Step-by-step:**

1. The proxy validates the API key, then calls `Balance::check(1)` (per RFC-0933 §5. Error Response, 402 PAYMENT_REQUIRED branch). The balance is read from Stoolap (CipherOcto's fork, `feat/blockchain-sql` branch).
2. Balance is `0` (or below the per-request cost in cents). The proxy returns **402 PAYMENT_REQUIRED** without touching the provider.
3. The client surfaces the 402 to the user. To continue, the buyer tops up via the billing flow (out of scope here — handled by the team budget dashboard).

**Why the `Balance` field, not the storage-layer `budget_limit`:** the proxy's 402 path reads the in-memory `Balance` field on the proxy state. The storage layer's per-key `budget_limit` field is validated only on key creation (rejects `budget_limit <= 0`). The two are intentionally separate: `budget_limit` is a hard cap (top-of-funnel), `Balance` is the running counter (per-request).

**Test pinned:** `e2e_wiremock_faults::test_budget_exhausted_returns_402` (mission `proxy-strong-scenarios-phase2`).

---

## Scenario 5 — Rate limit exceeded returns 429

The buyer is firing requests faster than the per-key token bucket allows. The proxy rejects with 429 and a `Retry-After` header.

```mermaid
sequenceDiagram
    participant C as Hardened client
    participant P as Local proxy
    participant TB as TokenBucket (DashMap)

    loop 10 fast requests
        C->>P: POST /v1/chat/completions
        P->>TB: bucket.try_consume(1)
        alt bucket has tokens
            TB-->>P: true
            P->>P: Forward to provider...
        else bucket empty
            TB-->>P: false
            P-->>C: 429 Too Many Requests<br/>Retry-After: 1
        end
    end
```

**Step-by-step:**

1. The proxy holds a per-key `TokenBucket` (RFC-0933). The bucket is refilled at the configured RPM (requests per minute) and consumed 1 per request.
2. On burst exhaustion, the proxy returns **429 Too Many Requests** with a `Retry-After: <seconds>` header.
3. The client backs off and retries. The hardened client SDK implements exponential jitter by default.
4. The bucket is per-API-key, not per-IP — different keys for the same IP do not share budget.

**Why two different "limit" mechanisms:** `Balance` (Scenario 4) is a _monetary_ cap; `TokenBucket` (this scenario) is a _rate_ cap. Both apply. A request must pass both to reach the provider.

**Note on streaming:** token bucket consumption is per-request, not per-token. A streaming response consuming 10k tokens still costs 1 bucket token. Cost-aware limiting uses `Balance`, not `TokenBucket`.

---

## Scenario 6 — Marketplace discovery: model not local

The buyer requests a model that has no `DispatchInfo` locally. The proxy queries the marketplace for sellers that offer it.

```mermaid
sequenceDiagram
    participant C as Hardened client
    participant P as Local proxy
    participant MK as Marketplace node (k/n)
    participant RR as Reputation registry

    C->>P: POST /v1/chat/completions<br/>model="claude-opus-4-5"
    P->>P: dispatch_map.get("claude-opus-4-5")
    alt match found
        P->>P: Forward to configured provider (Scenario 2)
    else no match
        P->>MK: DiscoverOffers {<br/>  model="claude-opus-4-5",<br/>  min_reputation=0.7,<br/>  max_price_cents=500<br/>}
        MK->>MK: List registered sellers offering model
        loop marketplace nodes (k out of n)
            MK->>RR: query_reputation(seller_did)
            RR-->>MK: reputation score + history
        end
        MK-->>P: Ranked offers [{<br/>  seller_did, price_cents,<br/>  reputation, latency_p50,<br/>  capacity_rpm<br/>}, ...]
        P-->>C: 300 Multiple Choices<br/>offers=[...]
    end
```

**Step-by-step:**

1. The proxy looks up the model in `dispatch_map`. No match.
2. The proxy queries the marketplace (`DiscoverOffers` request, RFC-0969 §Discovery). The marketplace nodes are queried in parallel via the DGP gossip substrate.
3. Each marketplace node checks its local reputation registry for sellers offering the requested model. The reputation score combines stake weight, performance history, and social validation (per the whitepaper §Proof of Reliability).
4. The marketplace returns a ranked list of offers. Each offer carries: seller DID, price per request (cents), reputation score, p50 latency, available capacity (RPM).
5. The proxy returns **300 Multiple Choices** with the offer list to the client. The client (or user) picks an offer.

**Why 300 (not 200):** returning 200 would imply the request was satisfied. Returning 503 would imply no providers. 300 lets the client distinguish "multiple offers available — pick one" from "no offers available — fall back".

**On the empty case:** if the marketplace returns zero offers (no seller serves this model), the proxy returns **503 SERVICE_UNAVAILABLE** with a different error body ("no marketplace offers for model X" — see consolidated 503 table above, row `marketplace-empty`). The client can then either retry later or reconfigure.

**Trust model:** the marketplace is gossip-based; k of n responses must agree on the offer set before the proxy trusts the rank. Disagreement (Sybil attempt) is caught at the gossip layer via the PoRelay trust registry.

---

## Scenario 7 — Deal: pick offer, mint capability token, fund escrow

The buyer picks an offer. The proxy mints a `CapabilityToken V2` envelope (per RFC-0957), funds an escrow, and prepares the mesh request.

```mermaid
sequenceDiagram
    participant C as Hardened client
    participant P as Local proxy
    participant W as Wallet node
    participant E as Escrow ledger

    C->>P: Pick offer { seller_did, max_price_cents }
    P->>W: MintCapabilityToken V2 {<br/>  audience=seller_did,<br/>  model="claude-opus-4-5",<br/>  max_price_cents=500,<br/>  expiry=now+5min,<br/>  caveats=[max_uses=1, valid_range]<br/>}
    W-->>P: CapabilityToken V2 (signed)
    P->>E: FundEscrow {<br/>  amount_cents=500,<br/>  capability_hash,<br/>  refund_on=["seller_offline", "capability_invalid"]<br/>}
    E-->>P: Escrow receipt
    P-->>C: { capability_token, escrow_receipt }
```

**Step-by-step:**

1. The client (or proxy on the client's behalf) picks one offer from the marketplace list.
2. The wallet node mints a CapabilityToken V2 (per RFC-0957). Caveats constrain the request: `max_uses=1` (one-shot, no replay), `valid_range` (time window), `max_price_cents` (price ceiling), and any node-specific caveats (e.g. data flagging).
3. The proxy funds an escrow on the buyer's behalf — the agreed price is locked in a vault keyed by `capability_hash`. Refund triggers are declared upfront.
4. The proxy now holds both a signed capability (to attach to the mesh request) and an escrow receipt (proof of payment intent).

**Why escrow upfront, not post-pay:** the seller node will spend real resources (provider API calls, sometimes non-refundable) before the buyer pays. Escrow converts the bilateral trust requirement (buyer trusts seller to deliver, seller trusts buyer to pay) into a unilateral trust requirement (each trusts the ledger).

**Why V2 envelope:** RFC-0957 v2 splits the envelope into a stable outer (`CapabilityBundleV2`) and an inner caveats structure. This lets router hops verify "this envelope authorizes X" without learning the inner caveats (privacy-preserving relay). V2 is the cutover target — V1 is deprecated but still accepted for compatibility during the migration window.

**Dual-mode (RFC-0959-A1):** capability tokens and legacy bearer tokens coexist. A request can carry either. The seller node accepts both and dispatches accordingly. Server-side market delivery uses capability-only (no bearer).

---

## Scenario 8 — Mesh forwarding: NodeEnvelope + HopSignature chain

The buyer's local proxy wraps the request in a `NodeEnvelope` and routes it through 2 router hops before it reaches the seller node.

```mermaid
sequenceDiagram
    participant P0 as Buyer proxy (hop 0)
    participant P1 as Router node A (hop 1)
    participant P2 as Router node B (hop 2)
    participant SN as Seller node (hop 3)

    P0->>P0: NodeEnvelope {<br/>  payload_kind=CHAT_REQUEST,<br/>  signer_did=buyer_did,<br/>  authorization=CapabilityToken V2,<br/>  body=OpusRequest<br/>}
    P0->>P0: hop_index=0, chain_hash=h0
    P0->>P0: Sign hop_signature over h0
    P0->>P1: envelope (over ORR onion route)
    P1->>P1: Verify hop_signature<br/>(hop 0 valid)
    P1->>P1: hop_index=1, chain_hash=h1<br/>Append P1 signature
    P1->>P2: envelope (next hop)
    P2->>P2: Verify P1 signature<br/>Append P2 signature
    P2->>SN: envelope (final hop)
    SN->>SN: Verify all signatures<br/>(hop 0, 1, 2 valid)
```

**Step-by-step:**

1. The buyer proxy constructs a `NodeEnvelope` (per RFC-0871 §Data Structures). Fields: `payload_kind` (one of the 7 RFC-0870 `PayloadKindId` UUIDs — `CHAT_REQUEST` here), `signer_did` (canonical `did:octo:z...` form), `authorization` (CapabilityToken V2), body bytes (the chat request).
2. The envelope is wrapped in an ORR (Onion Relay Routing, `architecture/octo-network-architecture.md §11`) layer so intermediate router hops see only the next hop, not the final destination.
3. At each hop, the router:
   - Strips one ORR layer to reveal the next hop.
   - Verifies the previous hop's `HopSignature` over the chain hash (per RFC-0871 §Data Structures). Invalid signature → drop.
   - Appends its own signature, increments `hop_index`, recomputes the chain hash.
   - Forwards.
4. The seller node receives the envelope with all signatures appended. It verifies the full chain (`hop_index = 0..n`, all signatures valid against the canonical chain hash).

**Why the chain hash:** it binds the envelope to the exact byte sequence of the request. Any modification en route (even a single byte) invalidates the chain. Replay protection comes from the capability token's `max_uses=1` caveat + the envelope's `envelope_id` (per RFC-0871 §TV3).

**Why onion routing:** the seller node does not learn the buyer's IP. Router hops do not learn the seller. Only the buyer's wallet knows the full route. Privacy by construction.

**Architecture reminder:** envelope construction lives in `octo-protocol` (Layer 1 substrate); onion routing lives in `octo-network`; provider dispatch lives in `quota-router-core`. See `CLAUDE.md §Architectural Principles` for the canonical layer model.

---

## Scenario 9 — Seller validation: capability verify, reputation check, provider dispatch

The seller node verifies the capability, checks the buyer's reputation, and forwards the request to its own configured provider.

```mermaid
sequenceDiagram
    participant SN as Seller node
    participant W as Wallet node (verify)
    participant ZK as ZK verifier (RFC-0958)
    participant RR as Reputation registry
    participant DP as dispatch_map (seller-side)
    participant ANT as Anthropic API (seller's key)

    SN->>W: VerifyCapability(V2 token)
    W->>ZK: Verify ZK proof of capability issuance
    ZK-->>W: valid
    W->>W: Check caveats:<br/>  - audience == seller_did<br/>  - expiry not passed<br/>  - model matches request<br/>  - max_uses not exceeded<br/>  - price <= max_price_cents
    W-->>SN: Capability valid
    SN->>RR: GetReputation(buyer_did)
    RR-->>SN: score = 0.85, history = clean
    SN->>SN: Apply seller-side rate limit + budget
    SN->>DP: dispatch_map.get("claude-opus-4-5")
    DP-->>SN: DispatchInfo { provider="anthropic" }
    SN->>ANT: POST /v1/messages (seller's API key)
    ANT-->>SN: SSE stream
```

**Step-by-step:**

1. The seller node's wallet verifies the CapabilityToken V2. This is a two-step process:
   - **ZK proof verification** (RFC-0958): the token was issued by a wallet node with valid signature, and the caveats hash commits to the declared caveats without revealing them.
   - **Caveat evaluation**: each caveat is checked against the request (audience, expiry, model, max_uses, price ceiling). Failure on any caveat → reject with a specific reason.
2. The seller node checks the buyer's reputation in the local reputation registry. Low reputation (below seller-configured threshold) → reject with **403 Forbidden** (emitted by the seller node, NOT the buyer's local proxy — `proxy.rs` does not synthesize 403 for reputation).
3. The seller node applies its own per-buyer rate limit and budget cap (preventing a malicious buyer from burning the seller's API quota).
4. The seller node's `dispatch_map` resolves the model to its own provider (Anthropic in this case). The seller uses their own API key — the buyer's request never sees it.
5. The seller proxies the request to Anthropic. Streams the response.

**Why dual verification (ZK + caveats):** the ZK proof binds the issuer; the caveats bind the content. Either alone is insufficient. ZK without caveats → token is real but might authorize anything. Caveats without ZK → token is bound but might be forged.

**Why reputation check:** reputation is a defense-in-depth layer against capability farming. A buyer who funds escrows but consistently triggers refunds will have low reputation, which other sellers can use to refuse service.

---

## Scenario 10 — Streaming response + settlement

The seller's provider streams SSE back through the mesh. The buyer proxy accumulates the stream, settles the escrow, and updates reputation.

```mermaid
sequenceDiagram
    participant ANT as Anthropic API
    participant SN as Seller node (hop 3)
    participant P2 as Router B (hop 2)
    participant P1 as Router A (hop 1)
    participant P0 as Buyer proxy (hop 0)
    participant E as Escrow ledger
    participant RR as Reputation registry

    ANT-->>SN: SSE: data: {"delta":"P"}
    SN-->>P2: SSE (forwarded hop 3→2)
    P2-->>P1: SSE (forwarded hop 2→1)
    P1-->>P0: SSE (forwarded hop 1→0)
    P0-->>P0: Accumulate tokens<br/>Append [DONE]
    P0->>P0: Forward SSE to client

    ANT-->>SN: SSE: data: [DONE]
    Note over SN,P0: [DONE] propagated

    P0->>E: escrow.settle {<br/>  capability_hash,<br/>  amount_cents=actual_tokens*rate,<br/>  splits=[seller: 90%, router_A: 4%, router_B: 4%, burn: 2%]<br/>}
    E-->>P0: Settlement receipt
    Note over P0: P0 orchestrates settle-then-record:<br/>escrow ledger does NOT auto-update reputation
    P0->>RR: marketplace.record_outcome(seller_did, success=true)
    P0->>RR: marketplace.record_outcome(router_A, success=true)
    P0->>RR: marketplace.record_outcome(router_B, success=true)
```

**Step-by-step:**

1. The seller's provider streams SSE. Each `data:` line is forwarded hop-by-hop back through the mesh. Router hops do not buffer — they stream through (back-pressure handled at the transport layer).
2. The buyer proxy accumulates the response, appends its own `[DONE]` terminator (one per request — the seller also appends one, so the buyer proxy dedups by counting), and forwards the stream to the client.
3. After `[DONE]` arrives (or the connection closes), the buyer proxy settles the escrow:
   - Computes actual cost based on tokens consumed (token count × per-token rate).
   - Splits payment: seller (majority), router hops (relay fee, ~4% each), network burn (small fraction).
   - Submits settlement to the ledger.
4. The reputation registry is updated for all participants. Positive outcomes increase score; the magnitude depends on stake weight and current score.

**Why streaming back through mesh is hard:** back-pressure must be end-to-end. A slow buyer cannot cause the seller to back up indefinitely. The mesh uses flow-controlled streams (RFC-0870 §Streaming Semantics) with bounded buffer per hop.

**Why settle post-stream not pre-stream:** pre-stream settlement requires committing to a price ceiling, which overcharges most requests. Post-stream settles the actual cost. The escrow exists to lock the ceiling; settlement realizes the actual.

**Why burn a fraction:** network fee (anti-spam). Without burn, the network has no defense against dust attacks (many tiny requests flooding the mesh).

---

## Scenario 11 — Adversarial: replay attack

An attacker captures a valid envelope and tries to replay it. The mesh rejects.

```mermaid
sequenceDiagram
    participant A as Attacker
    participant P1 as Router A
    participant SN as Seller node
    participant RR as Reputation registry

    A->>P1: Replay envelope (same envelope_id)
    P1->>SN: Forward (router doesn't know it's a replay)
    SN->>SN: Verify envelope_id against seen-set
    alt envelope_id seen before
        SN-->>P1: 409 Conflict<br/>{ "error": "envelope already processed" }
        Note over SN,RR: attacker_hop_id = PoRelay relay metadata<br/>(envelope's signer_did = legitimate buyer)
        SN->>RR: marketplace.record_outcome(attacker_hop_id, replay=true)
        RR-->>SN: attacker reputation -0.5
    else envelope_id new
        SN->>SN: Process normally
    end
```

**Step-by-step:**

1. The attacker captured a valid `NodeEnvelope` (e.g., from a compromised router hop, a misconfigured proxy log, or a side-channel).
2. The attacker submits the same envelope to the mesh, hoping the seller will process it twice (charging the original buyer's escrow twice).
3. The seller node maintains a `seen-set` of recently processed `envelope_id`s (Bloom filter or LRU cache, per RFC-0871 §TV3). The seen-set is bounded (~10k entries, evicted after 1 hour).
4. The seller detects the duplicate and returns **409 Conflict** (NOT 200). The escrow is NOT settled. (Wire-level seller-reject contract; `proxy.rs` does NOT synthesize 409 — the buyer's local proxy forwards the seller's 409 verbatim. No `StatusCode::CONFLICT` emit site in the buyer-side proxy code.)
5. The reputation registry records the replay attempt, reducing the attacker's reputation significantly.

**Why not prevent at the router:** router hops do not maintain the seen-set (would require global state). Only the seller, who has the strongest interest in preventing double-processing, maintains it.

**Why not just rely on `max_uses=1`:** the capability token's `max_uses` caveat covers single-seller single-use. Replay across different sellers (multi-seller replay) requires the seller-side seen-set. Both layers are needed.

**Why reputation penalty:** a single replay attempt is a strong signal of malicious intent. Buyers do not accidentally replay envelopes (the envelope_id is server-generated and the client SDK prevents reuse). A -0.5 reputation hit makes subsequent legitimate requests harder.

---

## Scenario 12 — Adversarial: seller offline mid-stream

The seller's node crashes mid-response. The mesh detects, the buyer gets a partial response + 502 + refund.

```mermaid
sequenceDiagram
    participant ANT as Anthropic API
    participant SN as Seller node (CRASH at t=2s)
    participant P2 as Router B
    participant P1 as Router A
    participant P0 as Buyer proxy
    participant C as Hardened client
    participant E as Escrow ledger

    ANT-->>SN: SSE: data: {"delta":"P"}
    SN-->>P2: SSE (forwarded)
    P2-->>P1: SSE (forwarded)
    P1-->>P0: SSE (forwarded)
    P0-->>P0: Client gets "P..."

    Note over SN: Crash at t=2s<br/>(SIGKILL or OOM)

    P2->>P2: Heartbeat timeout (5s)
    P2->>P1: ConnectionClosed
    P1->>P0: ConnectionClosed
    P0->>P0: Detect mid-stream failure<br/>Close SSE with 502 error event
    P0-->>C: 502 BAD_GATEWAY<br/>{ "error": "seller offline mid-stream",<br/>  "tokens_received": 1,<br/>  "refund_cents": 499 }
    P0->>E: RefundEscrow {<br/>  capability_hash,<br/>  reason="seller_offline",<br/>  refund_amount_cents=499<br/>}
    E-->>P0: Refund receipt
```

**Step-by-step:**

1. The seller's node crashes mid-response. The TCP connection from the seller to router B drops abruptly.
2. Router B detects the connection loss via its heartbeat timeout (5s default, configurable).
3. Router B signals router A, which signals the buyer proxy. The buyer proxy closes its SSE stream with a final 502 error event.
4. The client SDK detects the truncated stream and surfaces the partial response + error to the user.
5. The buyer proxy triggers the refund condition (`refund_on=["seller_offline", ...]` was declared in Scenario 7's escrow). The refund amount is the escrow total minus the per-token consumed (tokens received × per-token rate, ≈ 1 cent here).
6. The reputation registry records the seller's outage. Repeated outages cause reputation to decay below the marketplace threshold, eventually removing the seller from offer lists.

**Why mid-stream failures are harder than pre-stream:** pre-stream failures are atomic (no partial response). Mid-stream failures leave the buyer with a partial response and the seller with partial work done. The refund logic must price both fairly.

**Why heartbeat-based detection not TCP RST:** TCP RSTs are unreliable across NATs and onion routes. A heartbeat (`ping` every 1s, timeout at 5s) gives a bounded detection window independent of network topology.

**Why refund not retry:** retry would re-route to a different seller, but the buyer already saw partial output. Re-running with new seller would produce a different answer (LLM non-determinism). Better to surface the partial + refund and let the user retry manually with explicit context.

---

## Scenario 13 — Adversarial: capability fails server-side check

The capability passes the buyer's wallet check but fails the seller's caveat evaluation. The buyer disputes, the seller is slashed.

```mermaid
sequenceDiagram
    participant P0 as Buyer proxy
    participant SN as Seller node
    participant W as Wallet node (seller-side)
    participant D as Dispute registry
    participant E as Escrow ledger
    participant ST as PoRelay registry
    participant RR as Reputation registry

    P0->>SN: Submit request (envelope)
    SN->>W: VerifyCapability(V2)
    W->>W: Check caveats:<br/>  - audience: FAIL<br/>(capability bound to different seller)
    W-->>SN: InvalidCapability { reason: "audience mismatch" }
    SN-->>P0: 403 Forbidden<br/>{ "error": "capability invalid: audience mismatch" }

    P0->>D: OpenDispute {<br/>  capability_hash,<br/>  seller_did,<br/>  reason="invalid_capability",<br/>  evidence=capability_bytes<br/>}
    D->>E: escrow.dispute [state=Disputed]
    D->>ST: slashing.slash [seller_did, SlashReason::CapabilityForgery, 5_pct, pre-appeal]
    D->>RR: marketplace.record_outcome [seller_did, dispute=true]
```

**Step-by-step:**

1. The buyer's wallet minted the capability with `audience=seller_A_did`. The buyer's proxy accidentally routed the request to `seller_B_did` (or `seller_A` rotated keys and the audience is now stale).
2. The seller node's wallet fails the audience check. Returns **403 Forbidden** with a typed error (`audience mismatch`). (Emitted by the seller node — `proxy.rs` does not synthesize 403 for capability validation; it only maps incoming 403/401 to `RouterError::AuthError` via `proxy.rs::map_incoming_error_to_router_error`.)
3. The buyer proxy opens a dispute, attaching the capability bytes as evidence.
4. The dispute registry:
   - Refunds the buyer's escrow in full (the seller did not perform work).
   - Slashes the seller's stake by 5% (not the full stake — partial slash reserves room for honest mistakes vs malicious behavior).
   - Updates reputation: -0.3 for the seller.
5. The seller can appeal by submitting a counter-claim (e.g., "the buyer sent the wrong audience"). Appeals are processed by a randomly selected k/n of marketplace nodes (per Scenario 6's gossip quorum model), NOT the reputation registry — the registry is read-only here.

**Why typed error not generic 403:** the buyer needs to distinguish "audience mismatch" (rotated key, retry with new capability) from "exceeded max_uses" (capability spent, don't retry) from "invalid ZK proof" (token is forged, don't retry). Typed errors drive the right client behavior.

**Why partial slash not full slash:** a full slash on first offense destroys economic incentives for new sellers (one mistake = bankrupt). The 5% slash + reputation decay creates a gradient: honest sellers recover, persistent offenders exit the marketplace.

**Why dispute needs evidence:** without evidence, the buyer could dispute every request to drain the seller's stake. The capability bytes are the cryptographic proof of what was promised.

---

## Scenario 14 — Adversarial: Sybil seller with fake reputation

A bad actor creates many seller identities (Sybil attack) and tries to dominate the marketplace offer list with fake high-reputation sellers.

```mermaid
sequenceDiagram
    participant A as Attacker
    participant MK as Marketplace node (k/n)
    participant RR as Reputation registry
    participant ST as PoRelay registry
    participant P0 as Buyer proxy

    A->>A: Create 1000 seller DIDs<br/>Mint cheap stake on each
    A->>MK: DiscoverOffers { model="claude-opus-4-5" }
    MK->>RR: query_reputation(all 1000)
    loop for each Sybil DID
        RR->>ST: get_stake(seller_did)
        ST-->>RR: stake_cents
        RR->>RR: get_history(seller_did)
    end
    RR->>RR: Aggregate: stake_weight × history_length
    RR-->>MK: All 1000 Sybils rank LOW<br/>(stake per identity is tiny)
    MK-->>P0: Top offers are honest sellers<br/>(high stake, long history)
```

**Step-by-step:**

1. Attacker mints 1000 seller DIDs, each funded with the minimum stake (e.g., $1).
2. Attacker submits fake performance data for each Sybil: "10000 requests, 100% success rate".
3. The reputation registry computes a weighted score where the dominant factors are:
   - **Stake weight** (log-scaled, $1 × 1000 = $1000 is much less than $10000 × 1 honest seller).
   - **History length** (Sybil is 1 day old; honest seller is 2 years old).
   - **Stake-to-volume ratio** (Sybil: high request count on tiny stake → red flag).
4. The honest sellers outrank the Sybils in every discovery query. The marketplace's offer list is dominated by honest sellers.

**Why Sybil resistance is in the registry not the gossip layer:** gossip can be Sybil'd at the network layer (cheap to run many nodes). Reputation requires economic commitment (stake) that scales with influence. The registry is the right place to apply Sybil resistance.

**Why stake weight is log-scaled:** linear scaling lets $1M attacker dominate $100k honest actor (10×). Log scaling makes the attacker's advantage sublinear: $1M vs $100k honest = log advantage, not 10×.

**Why history length matters:** new sellers (even honest ones) cannot dominate. They must earn reputation over time. This is the social validation component of PoR.

**What if the attacker has more capital than all honest sellers?** Then they are no longer Sybil — they are a dominant single actor. The marketplace still routes to them, but the system is now a cartel. This is the anti-fragility limit: a sufficiently capitalized attacker can dominate any PoS-style system. Mitigation is off-chain governance (community can refuse to use the marketplace).

---

## Open questions

These gaps surfaced while writing this doc. Each should become either a follow-on mission or an RFC amendment.

1. **CapabilityToken V2 caveat schema** — RFC-0957 specifies the v2 envelope but does not enumerate the full caveat set. RFC-0965 §Caveat type enumeration defines 10 caveat variants across v1.0 (Vault, Permission, ValidAfter, MaxUses, AuditWindow, RedemptionContext, WrappedOnly, Factory, Sharded) and v1.1 (PolicyReference) — mission `0965-a-caveat-dsl`. Note: `ValidRange` and `MaxPerTx` are NOT RFC-0965 caveats; they are RFC-0964 `Constraint` envelope variants. This list should be cross-referenced from the deal scenario.
2. **Settlement token unit** — Scenario 10 settles in cents. The actual ledger (stoolap off-chain vs on-chain) and the canonical currency (USD-pegged stablecoin vs OCTO token) is open.
3. **Mid-stream refund formula** — Scenario 12 uses `tokens_received × per_token_rate`. The actual formula needs a spec (does the first token cost more? does context length matter?).
4. **Dispute appeal SLA** — Scenario 13 mentions appeals but does not specify the SLA. Needs an RFC amendment.
5. **Onion route key rotation** — Scenario 8 uses ORR but does not specify how the route keys rotate. The ORR section in `octo-network-architecture.md` mentions mission key hierarchy but not rotation cadence.

## Related RFCs

| RFC         | Title                                 | Used in scenarios                             |
| ----------- | ------------------------------------- | --------------------------------------------- |
| RFC-0104    | Deterministic Floating-Point          | (cross-cutting, not in any scenario directly) |
| RFC-0862    | Substrate Types                       | 8 (NodeEnvelope construction)                 |
| RFC-0870    | NodeEnvelope Adoption                 | 8, 10                                         |
| RFC-0871    | Specialized Node Protocol Envelope    | 8, 11                                         |
| RFC-0964    | Capability Constraints                | 7, 9 (ValidRange / MaxPerTx envelope types)   |
| RFC-0909    | Deterministic Quota Accounting        | 2, 9                                          |
| RFC-0917    | Mode Gate                             | 1, 6 (always HTTP proxy + SDK)                |
| RFC-0933    | Rate Limiting Integration             | 1, 4, 5                                       |
| RFC-0957    | Capability Token V2                   | 7, 9, 11, 13                                  |
| RFC-0958    | ZK Capability                         | 9                                             |
| RFC-0959-A1 | Dual-Mode Auth                        | 7                                             |
| RFC-0969    | Dual Pipeline                         | 6, 7                                          |
| RFC-0970    | Forwarding-Hop Authorization Envelope | 8                                             |
| RFC-0971    | Destination-Node Role Consolidation   | 9                                             |

> RFC-0943 ("Team Budget") is referenced in some code comments but **no RFC-0943 file exists** — the file was never filed. Treat any `RFC-0943` reference in code as a forward-ref to RFC-0933 (Rate Limiting) which absorbs team-budget semantics via the `Balance` field. A separate RFC-0943 filing mission is filed under `missions/archived/0943-b-per-team-budgets.md`.

## Related missions

| Mission                                   | Status             | Touches scenarios                                 |
| ----------------------------------------- | ------------------ | ------------------------------------------------- |
| `proxy-strong-scenarios`                  | LANDED 2026-08-12  | 1-5                                               |
| `proxy-strong-scenarios-phase2`           | LANDED 2026-08-13  | 3, 4                                              |
| `0870k-transport-request-response`        | CLAIMED 2026-08-12 | 8, 10 (substrate)                                 |
| `0871b-cross-node-forwarding`             | LANDED 2026-08-12  | 8, 11 (hop signature)                             |
| `0871b-storage-backend`                   | LANDED 2026-08-11  | 4 (balance storage)                               |
| `0010-f2-multi-chain-did-resolution`      | LANDED 2026-08-11  | 6 (chain-aware DID)                               |
| `0010-f8-rich-did-documents`              | LANDED 2026-08-11  | 6, 9 (rich DID for reputation)                    |
| `0010-f8-rich-did-storage`                | LANDED 2026-08-11  | 6 (storage layer)                                 |
| `marketplace-escrow-caller-authorization` | LANDED 2026-08-13  | 6, 7 (escrow caller auth — Round 1 review C1 fix) |
| `marketplace-e2e-strong-scenarios`        | LANDED 2026-08-13  | 6, 7, 10, 12, 13, 14                              |
| `0957-phase2b-payment-caveat`             | LANDED 2026-08-13  | 7 (PaymentCaveat + macaroon HMAC)                 |
| `0957-phase2c-capability-issuer-wiring`   | LANDED 2026-08-13  | 7 (Capability Issuer Node wiring)                 |

> **Phantom pointer alert:** `missions/open/marketplace-e2e-strong-scenarios.md` is a duplicate of the claimed file (per `no-phantom-mission-pointers` memory). The duplicate should be deleted in a follow-up sweep; this doc cites the `claimed/` file (the canonical version).

## Test coverage map

| Scenario           | Existing tests                                                                                                                                                                                                                                                                                    | Tests needed                                                          |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------- |
| 1 Hello world      | `e2e_proxy::test_chat_completion_basic` (e2e_proxy.rs)                                                                                                                                                                                                                                            | —                                                                     |
| 2 Multi-provider   | `e2e_wiremock_faults::test_dispatch_map_no_match_returns_503` (covers the asymmetric-guard path)                                                                                                                                                                                                  | Multi-provider resolution unit tests (positive match path)            |
| 3 Provider 500     | `e2e_wiremock_faults::test_upstream_500_returns_502`, `test_no_fallback_config_upstream_500_surfaces_as_502`, `proxy::test_post_dispatch_5xx_triggers_fallback` (lib)                                                                                                                             | Fallback dance test (already covered by lib test, not by wiremock)    |
| 4 Budget 402       | `e2e_wiremock_faults::test_budget_exhausted_returns_402`                                                                                                                                                                                                                                          | —                                                                     |
| 5 Rate limit       | `e2e_proxy::test_rpm_rate_limit_returns_429` (e2e_proxy.rs), `key_rate_limiter::tests::test_token_bucket_basic` (lib)                                                                                                                                                                             | Wiremock pinning of `Retry-After: <seconds>` header value             |
| 6 Marketplace      | `marketplace_e2e` lib tests (27 tests pinning escrow/dispute/settlement invariants — `marketplace-e2e-strong-scenarios` mission); `crates/octo-network/tests/gdp_discovery.rs` + `gdp_deep.rs` cover the underlying GDP substrate used by marketplace nodes (NOT the marketplace-e2e flow itself) | Wiremock-style Sybil resistance at marketplace layer                  |
| 7 Deal + escrow    | `0957-phase2b-payment-caveat` unit tests                                                                                                                                                                                                                                                          | CapabilityToken V2 caveat exhaustiveness across all 9 caveat variants |
| 8 Mesh forwarding  | `0871b-cross-node-forwarding` (partial TV)                                                                                                                                                                                                                                                        | 3-node end-to-end TV (awaits `0870k`)                                 |
| 9 Seller validate  | `0957-phase2b-payment-caveat` lib tests                                                                                                                                                                                                                                                           | ZK verification + reputation + provider dispatch integration          |
| 10 Stream + settle | `e2e_wiremock_faults::test_streaming_response_carries_events`, `test_streaming_upstream_500_returns_502`                                                                                                                                                                                          | Mesh-streaming back-pressure (no test currently)                      |
| 11 Replay          | (none)                                                                                                                                                                                                                                                                                            | Seen-set Bloom filter test                                            |
| 12 Seller offline  | `e2e_wiremock_faults::test_streaming_upstream_500_returns_502` (partial)                                                                                                                                                                                                                          | Mid-stream TCP drop simulation (separate from upstream 500 path)      |
| 13 Dispute         | (covered indirectly by `marketplace-escrow-caller-authorization` lib tests)                                                                                                                                                                                                                       | Appeal flow (no dedicated test yet)                                   |
| 14 Sybil           | (network-layer tests in `crates/octo-network/tests/porelay_proofs.rs`)                                                                                                                                                                                                                            | Stake-weighted Sybil resistance at scale (1000+ identities)           |

> **Note on `marketplace_e2e::test_*`:** `marketplace_e2e.rs` exists at `crates/quota-router-core/tests/marketplace_e2e.rs` with 24 e2e tests (14 listed in mission `marketplace-e2e-strong-scenarios` ACs + 10 legacy/baseline tests). Named examples: `happy_path_bid_matches_ask_escrow_settles`, `dispute_valid_slashes_seller`, `escrow_recovery_from_locked_state_succeeds`, `concurrent_settlement_duplicate_rejected`. The coverage map above reflects this.

## Change log

| Date       | Change                                      | Author          |
| ---------- | ------------------------------------------- | --------------- |
| 2026-08-13 | Initial draft, 14 scenarios across 8 phases | cc (brainstorm) |
