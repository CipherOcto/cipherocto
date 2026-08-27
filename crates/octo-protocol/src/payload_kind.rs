//! Payload kind discriminator (RFC-0871 §Data Structures).
//!
//! 128-bit UUID per RFC-0965 caveat discriminator pattern (16 bytes instead of 1).
//! Old code fails-closed on unknown discriminators (RFC-0965 §3.2 pattern).
//! No central enum: each new payload kind = new RFC + new UUID allocation.

use borsh::{BorshDeserialize, BorshSerialize};

/// 128-bit payload discriminator (UUID-shaped).
///
/// RFC-0871 §Data Structures. Wire form is a flat 16-byte big-endian UUID.
/// RFC-allocated namespace + user-extension range; see [`rfc_namespace`] /
/// [`user_extension_range`] / [`capability_extension_range`].
///
/// No `Display` / `FromStr` is provided — `PayloadKindId` is opaque on the
/// wire and meaningful only in the context of an RFC-allocated range. Cross-
/// mission identifiers are exchanged via their human-readable RFC-XXXX number,
/// not via the wire bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct PayloadKindId(pub [u8; 16]);

impl PayloadKindId {
    /// Wrap a 16-byte buffer. No validation — caller must ensure the bytes
    /// were sourced from an RFC-allocated range or user-extension registration.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Borrow the inner 16-byte buffer.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// True if the discriminator sits in the RFC-allocated namespace
    /// (`0x0009_0000` … `0x0009_FFFF` for identity-substrate RFCs, the historical
    /// placeholder used by RFC-0871 §Test Vectors TV1).
    #[must_use]
    pub fn is_rfc_allocated(&self) -> bool {
        rfc_namespace().contains(&self.0)
    }

    /// True if the discriminator sits in the capability-extension namespace
    /// (RFC-0965 reserved range `0x0010_0000` … `0x0010_FFFF`).
    #[must_use]
    pub fn is_capability_extension(&self) -> bool {
        capability_extension_range().contains(&self.0)
    }

    /// True if the discriminator sits in the user-extension namespace
    /// (`0xFFFF_FF00` … `0xFFFF_FFFF`).
    #[must_use]
    pub fn is_user_extension(&self) -> bool {
        user_extension_range().contains(&self.0)
    }
}

/// RFC-allocated namespace (`0x0009_0000_0000_0000_0000_0000_0000_0000`
/// … `0x0009_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF`, first 16 bits = `0x0009`).
///
/// `0x0009` is the historical placeholder used by RFC-0871 §TV1 — concrete
/// sub-ranges are allocated per-RFC.
pub const fn rfc_namespace() -> RangeU128 {
    RangeU128 {
        start: 0x0009_0000_0000_0000_0000_0000_0000_0000,
        end: 0x0009_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF,
    }
}

/// Capability-extension namespace (RFC-0965 reserved range, first 16 bits = `0x0010`).
pub const fn capability_extension_range() -> RangeU128 {
    RangeU128 {
        start: 0x0010_0000_0000_0000_0000_0000_0000_0000,
        end: 0x0010_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF,
    }
}

/// User-extension namespace (last 256 values of UUID space).
pub const fn user_extension_range() -> RangeU128 {
    RangeU128 {
        start: 0xFFFF_FF00_0000_0000_0000_0000_0000_0000,
        end: 0xFFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF_FFFF,
    }
}

/// Inclusive-exclusive range over the 128-bit UUID space.
#[derive(Clone, Copy, Debug)]
pub struct RangeU128 {
    /// Inclusive start.
    pub start: u128,
    /// Inclusive end.
    pub end: u128,
}

impl RangeU128 {
    /// True if `value` falls within the range.
    #[must_use]
    pub fn contains(&self, bytes: &[u8; 16]) -> bool {
        let v = u128::from_be_bytes(*bytes);
        v >= self.start && v <= self.end
    }
}

/// Identity-resolve payload kind (RFC-0871 §Test Vectors TV1).
///
/// UUID: `0x0009:0001:0000:0000:0000:0000:0000:0001`
pub const IDENTITY_RESOLVE: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
]);
/// Companion `def_line` for `IDENTITY_RESOLVE` (compiler-resolved via `line!()`).
pub(crate) const __DEF_LINE_IDENTITY_RESOLVE: u32 = line!();

// RFC-0871 identity-write payload kinds (RFC-0862 §DidWriteCoordinator,
// mission 0871e-f7-impl-resolver-mediation). The resolver-node mediates
// `register` / `revoke` calls through an injected
// `Arc<dyn DidWriteCoordinator>` (Layer B substrate in `octo-ident`)
// before delegating to the local `DidRegistry` backend. Wire form:
// borsh-encoded `(canonical_did: String, document: DidDocument)`.
//
// Sub-namespace continues the identity allocation pattern: `0x0001`.
// New UUIDs `0002` (register) + `0003` (revoke).

/// Identity-register payload kind (RFC-0862 §DidWriteCoordinator).
///
/// UUID: `0x0009:0001:0000:0000:0000:0000:0000:0002`
pub const IDENTITY_REGISTER: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02,
]);
/// Companion `def_line` for `IDENTITY_REGISTER` (compiler-resolved via `line!()`).
pub(crate) const __DEF_LINE_IDENTITY_REGISTER: u32 = line!();

/// Identity-revoke payload kind (RFC-0862 §DidWriteCoordinator).
///
/// UUID: `0x0009:0001:0000:0000:0000:0000:0000:0003`
pub const IDENTITY_REVOKE: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03,
]);
/// Companion `def_line` for `IDENTITY_REVOKE` (compiler-resolved via `line!()`).
pub(crate) const __DEF_LINE_IDENTITY_REVOKE: u32 = line!();

// RFC-0871 cross-domain resolution payload kind (mission
// 0871b-cross-domain-resolution-impl). Allocated in the RFC-0871
// `rfc_namespace` (`0x0009:...`) with sub-namespace `0x0001` (mission
// 0871 — identity) and slot `0004` (next free slot after RESOLVE 0001,
// REGISTER 0002, REVOKE 0003). Wire form: borsh-encoded
// `ChainResolveRequest { target: String, hops: Vec<ResolverHop>,
// ttl_remaining_ms: u64 }`.
//
// Cross-node forwarding (network call from hop N to hop N+1) requires
// the request/response substrate that does not yet exist in
// `octo-transport` (only `broadcast` and `send_best` fire-and-forget).
// This payload kind carries the chain-traversal LOGIC substrate
// (cycle detection + TTL budget + ordered hop list). A follow-on
// mission wires a network-capable `ResolveChainHandler` when the
// request/response substrate lands.

/// Identity-resolve-chain payload kind (RFC-0871 §Future Work, mission
/// 0871b-cross-domain-resolution-impl).
///
/// UUID: `0x0009:0001:0000:0000:0000:0000:0000:0004`
pub const IDENTITY_RESOLVE_CHAIN: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04,
]);
/// Companion `def_line` for `IDENTITY_RESOLVE_CHAIN` (compiler-resolved via `line!()`).
pub(crate) const __DEF_LINE_IDENTITY_RESOLVE_CHAIN: u32 = line!();

/// Identity-resolve-with-chain payload kind (RFC-0010 §ChainId
/// Namespace Extension, mission `0010-f2-multi-chain-routing`).
///
/// Carries an explicit `ChainId` namespace in the request so the
/// resolver can route to a specific chain namespace on a multi-chain
/// deployment. Distinct from `IDENTITY_RESOLVE` (single-chain, mainnet
/// default) and `IDENTITY_RESOLVE_CHAIN` (chain-of-resolvers, not
/// chain-of-DIDs).
///
/// UUID: `0x0009:0001:0000:0000:0000:0000:0000:0005`
pub const IDENTITY_RESOLVE_WITH_CHAIN: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05,
]);
/// Companion `def_line` for `IDENTITY_RESOLVE_WITH_CHAIN` (compiler-resolved via `line!()`).
pub(crate) const __DEF_LINE_IDENTITY_RESOLVE_WITH_CHAIN: u32 = line!();

/// Identity-resolve-chain-response payload kind (RFC-0871 §Future Work,
/// mission `0870k-transport-request-response` AC-6).
///
/// Allocated in the RFC-0871 `rfc_namespace` (`0x0009:...`) with
/// sub-namespace `0x0001` (mission 0871 — identity) and slot `:0006`
/// (next free slot after RESOLVE :0001, REGISTER :0002, REVOKE :0003,
/// CHAIN :0004, WITH_CHAIN :0005). The cross-network chain reply
/// envelope carries this payload kind so the wrapping node can
/// distinguish the response from the request at the dispatch boundary.
///
/// Wire form: borsh-encoded `ChainResolveResponse` (canonical_did,
/// public_key, hops_traversed, signature_chain, envelope_id — 5-tuple
/// per RFC-0871 §Algorithms step 4).
///
/// Distinct from `IDENTITY_RESOLVE_CHAIN` (the request payload kind).
/// The two share the same wire schema envelope but the dispatcher
/// routes by `payload_kind` to know which decoder to apply.
///
/// UUID: `0x0009:0001:0000:0000:0000:0000:0000:0006`
pub const IDENTITY_RESOLVE_CHAIN_RESPONSE: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x06,
]);
/// Companion `def_line` for `IDENTITY_RESOLVE_CHAIN_RESPONSE` (compiler-resolved via `line!()`).
pub(crate) const __DEF_LINE_IDENTITY_RESOLVE_CHAIN_RESPONSE: u32 = line!();

#[cfg(test)]
mod reserved_slot_tests {
    use super::*;

    /// Mission `0870k-transport-request-response` AC-6: the public
    /// `IDENTITY_RESOLVE_CHAIN_RESPONSE` payload kind exists at UUID
    /// `0x0009:0001:0000:0000:0000:0000:0000:0006` (slot `:0006` in the
    /// identity sub-namespace) and is pairwise distinct from every
    /// other allocated payload kind in this file. The previous
    /// "reserved sentinel" const + `reserved_slot_0006_not_allocated`
    /// test was deleted along with the wire-dead reservation (the
    /// reservation was at the wrong UUID — sub-namespace `:0006` instead
    /// of slot `:0006`); the LIVE allocation now lands in the
    /// documented identity slot.
    ///
    /// SCOPE (round-4 R1 finding): this scan is
    /// **compile-unit-local** — it only enumerates constants visible
    /// in THIS translation unit. A future payload-kind constant
    /// added to **a different crate**, or added to this crate but
    /// outside this file (separate translation unit), without the
    /// `known` array being updated would not trip this guard.
    /// Cross-crate / cross-translation-unit protection would
    /// require a workspace-wide `cargo metadata`-driven build
    /// script or a workspace-level `tests/` integration test;
    /// deferred (out of round-4 scope).
    ///
    /// COVERAGE (round-5 R1 finding): the `known` array below
    /// enumerates **22 entries total** of `PayloadKindId` constants
    /// defined in this file (6 IDENTITY + 4 WALLET + 7 QUOTA +
    /// 1 REPUTATION + 3 CAPABILITY + 1 PAID_QUERY = 22). Adding a
    /// new constant in this file without updating the `known` array
    /// turns this test into a fail-closed guard for the new
    /// addition.
    ///
    /// DEFLINE-DRIFT (round-8 R2 fix): each entry's `def_line: u32`
    /// references a `pub(crate) const __DEF_LINE_X: u32 = line!();`
    /// companion co-located with its `pub const X: PayloadKindId`.
    /// The companion captures the source line of itself at compile
    /// time, so the diagnostic's `(defined at payload_kind.rs:{def_line})`
    /// always points at a line near the actual const definition
    /// (companion is ~3 lines below `pub const X`; maintainer scrolls
    /// up to find the def). This eliminates the manual `def_line: u32`
    /// literal-drift class that round-7 patched (16/21 stale values
    /// off by +19 lines) and which round-2 originally flagged as
    /// HIGH coupling fragility. **Why option (b) over (a):** Layer A
    /// constraint (RFC-frozen, years-stable) rules out a `build.rs` +
    /// `syn`-generated approach (toolchain surface area); `line!()`
    /// is core Rust stable since 1.0 and adds zero deps.
    #[test]
    fn identity_resolve_chain_response_split_from_request() {
        // AC-6 (mission 0870k-transport-request-response):
        // `IDENTITY_RESOLVE_CHAIN_RESPONSE` (the cross-network reply
        // payload kind) is allocated at UUID
        // `0x0009:0001:0000:0000:0000:0000:0000:0006`. The companion
        // request payload kind `IDENTITY_RESOLVE_CHAIN` sits at slot
        // `:0004` (UUID `0x0009:0001:0000:0000:0000:0000:0000:0004`).
        // The two MUST be distinct so the dispatcher can route reply
        // vs request to the right decoder.
        assert_ne!(
            IDENTITY_RESOLVE_CHAIN_RESPONSE, IDENTITY_RESOLVE_CHAIN,
            "IDENTITY_RESOLVE_CHAIN_RESPONSE must differ from IDENTITY_RESOLVE_CHAIN"
        );
        // Assert the new constant sits at the documented UUID slot
        // `:0006` in the identity sub-namespace.
        let expected: [u8; 16] = [
            0x00, 0x09, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x06,
        ];
        assert_eq!(
            IDENTITY_RESOLVE_CHAIN_RESPONSE.0, expected,
            "IDENTITY_RESOLVE_CHAIN_RESPONSE must live at slot :0006 of identity sub-namespace"
        );
    }

    /// Round-3 review (C3): asserts the new `IDENTITY_RESOLVE_CHAIN_RESPONSE`
    /// payload kind does not collide with any of the 21 other known
    /// payload-kind constants in this file. If a future mission
    /// allocates this slot for an unrelated purpose (e.g. an
    /// unrelated `*_PAYLOAD_KIND` const), this test fails before the
    /// collision reaches the wire.
    #[test]
    fn identity_resolve_chain_response_distinct_from_known() {
        let target = IDENTITY_RESOLVE_CHAIN_RESPONSE;
        // Each entry carries the `pub const X` definition line (via
        // compiler-resolved `__DEF_LINE_X` companion const) so a
        // test-failure message can point the maintainer at the source
        // site (round-6 R1 finding: cross-ref for navigation; round-8
        // R2 fix: drift-free via `line!()`).
        let known: &[(&str, PayloadKindId, u32)] = &[
            // IDENTITY sub-namespace 0x0009:0001:...
            (
                "IDENTITY_RESOLVE",
                IDENTITY_RESOLVE,
                __DEF_LINE_IDENTITY_RESOLVE,
            ),
            (
                "IDENTITY_REGISTER",
                IDENTITY_REGISTER,
                __DEF_LINE_IDENTITY_REGISTER,
            ),
            (
                "IDENTITY_REVOKE",
                IDENTITY_REVOKE,
                __DEF_LINE_IDENTITY_REVOKE,
            ),
            (
                "IDENTITY_RESOLVE_CHAIN",
                IDENTITY_RESOLVE_CHAIN,
                __DEF_LINE_IDENTITY_RESOLVE_CHAIN,
            ),
            (
                "IDENTITY_RESOLVE_WITH_CHAIN",
                IDENTITY_RESOLVE_WITH_CHAIN,
                __DEF_LINE_IDENTITY_RESOLVE_WITH_CHAIN,
            ),
            // WALLET sub-namespace 0x0009:0002:...
            (
                "WALLET_SIGN_ED25519",
                WALLET_SIGN_ED25519,
                __DEF_LINE_WALLET_SIGN_ED25519,
            ),
            (
                "WALLET_MINT_CAPABILITY",
                WALLET_MINT_CAPABILITY,
                __DEF_LINE_WALLET_MINT_CAPABILITY,
            ),
            (
                "WALLET_ATTENUATE_CAPABILITY",
                WALLET_ATTENUATE_CAPABILITY,
                __DEF_LINE_WALLET_ATTENUATE_CAPABILITY,
            ),
            (
                "WALLET_RESOLVE_DID",
                WALLET_RESOLVE_DID,
                __DEF_LINE_WALLET_RESOLVE_DID,
            ),
            // QUOTA sub-namespace 0x0009:0003:...
            (
                "QUOTA_ROUTER_ANNOUNCE",
                QUOTA_ROUTER_ANNOUNCE,
                __DEF_LINE_QUOTA_ROUTER_ANNOUNCE,
            ),
            (
                "QUOTA_ROUTER_WITHDRAW",
                QUOTA_ROUTER_WITHDRAW,
                __DEF_LINE_QUOTA_ROUTER_WITHDRAW,
            ),
            (
                "QUOTA_CAPACITY_GOSSIP",
                QUOTA_CAPACITY_GOSSIP,
                __DEF_LINE_QUOTA_CAPACITY_GOSSIP,
            ),
            (
                "QUOTA_CAPACITY_REQUEST",
                QUOTA_CAPACITY_REQUEST,
                __DEF_LINE_QUOTA_CAPACITY_REQUEST,
            ),
            (
                "QUOTA_FORWARD_REQUEST",
                QUOTA_FORWARD_REQUEST,
                __DEF_LINE_QUOTA_FORWARD_REQUEST,
            ),
            (
                "QUOTA_FORWARD_RESPONSE",
                QUOTA_FORWARD_RESPONSE,
                __DEF_LINE_QUOTA_FORWARD_RESPONSE,
            ),
            (
                "QUOTA_FORWARD_REJECT",
                QUOTA_FORWARD_REJECT,
                __DEF_LINE_QUOTA_FORWARD_REJECT,
            ),
            // REPUTATION sub-namespace 0x0009:0004:...
            (
                "REPUTATION_ANCHOR_QUERY",
                REPUTATION_ANCHOR_QUERY,
                __DEF_LINE_REPUTATION_ANCHOR_QUERY,
            ),
            // CAPABILITY sub-namespace 0x0009:0005:...
            (
                "CAPABILITY_ISSUE",
                CAPABILITY_ISSUE,
                __DEF_LINE_CAPABILITY_ISSUE,
            ),
            (
                "CAPABILITY_REVOKE",
                CAPABILITY_REVOKE,
                __DEF_LINE_CAPABILITY_REVOKE,
            ),
            (
                "CAPABILITY_LOOKUP",
                CAPABILITY_LOOKUP,
                __DEF_LINE_CAPABILITY_LOOKUP,
            ),
            // PAID_QUERY sub-namespace 0x0009:0006:...
            (
                "PAID_QUERY_VERIFY",
                PAID_QUERY_VERIFY,
                __DEF_LINE_PAID_QUERY_VERIFY,
            ),
        ];
        for (name, kind, def_line) in known {
            // Round-6 R1 finding: include the colliding constant's
            // UUID bytes + the definition file:line in the failure
            // message so a maintainer can diagnose without re-reading
            // the file by hand.
            assert_ne!(
                kind.0,
                target.0,
                "slot :0006 chain-response collision: {name} (defined at payload_kind.rs:{def_line}) collides with IDENTITY_RESOLVE_CHAIN_RESPONSE; colliding UUID bytes = {:?}; target UUID bytes = {:?}",
                kind.0,
                target.0,
            );
        }
    }
}

/// Wallet sign Ed25519 (RFC-0871 §Wallet Node Lifecycle, Phase 2 mission 0871a).
///
/// UUID: `0x0009:0002:0000:0000:0000:0000:0000:0001`
pub const WALLET_SIGN_ED25519: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
]);
/// Companion `def_line` for `WALLET_SIGN_ED25519` (compiler-resolved via `line!()`).
pub(crate) const __DEF_LINE_WALLET_SIGN_ED25519: u32 = line!();

/// Wallet mint capability (RFC-0871 §Wallet Node Lifecycle, Phase 2 mission 0871a).
///
/// UUID: `0x0009:0002:0000:0000:0000:0000:0000:0002`
pub const WALLET_MINT_CAPABILITY: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02,
]);
/// Companion `def_line` for `WALLET_MINT_CAPABILITY` (compiler-resolved via `line!()`).
pub(crate) const __DEF_LINE_WALLET_MINT_CAPABILITY: u32 = line!();

/// Wallet attenuate capability (RFC-0871 §Wallet Node Lifecycle, Phase 2 mission 0871a).
///
/// UUID: `0x0009:0002:0000:0000:0000:0000:0000:0003`
pub const WALLET_ATTENUATE_CAPABILITY: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03,
]);
/// Companion `def_line` for `WALLET_ATTENUATE_CAPABILITY` (compiler-resolved via `line!()`).
pub(crate) const __DEF_LINE_WALLET_ATTENUATE_CAPABILITY: u32 = line!();

/// Wallet resolve DID (RFC-0871 §Test Vectors TV7).
///
/// UUID: `0x0009:0002:0000:0000:0000:0000:0000:0004`
pub const WALLET_RESOLVE_DID: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04,
]);
/// Companion `def_line` for `WALLET_RESOLVE_DID` (compiler-resolved via `line!()`).
pub(crate) const __DEF_LINE_WALLET_RESOLVE_DID: u32 = line!();

// RFC-0870 quota router payload kinds (RFC-0870 §NodeEnvelope Adoption,
// mission 0870-b-envelope-adoption). Allocated in the RFC-0871
// `rfc_namespace` (`0x0009:...`) with sub-namespace `0x0003` (RFC-0870 — quota
// router mesh). The RFC-0870 amendment table uses conceptual `[0x87, 0x00, ...]`
// byte placeholders; this allocation is the canonical materialization per
// RFC-0871 §Ordering + RFC-0871 §Namespace. Mission 0870-b maps the legacy
// discriminator bytes (0xC3–0xCB) to these UUIDs at the quota-router-core
// boundary (see `crates/quota-router-core/src/node/envelope_v2.rs`).
//
// RFC-0870 §NodeEnvelope Adoption table:
//
// | Legacy discriminator | New PayloadKindId                |
// |----------------------|-----------------------------------|
// | 0xC3 (FWD_REQUEST)   | QUOTA_FORWARD_REQUEST             |
// | 0xC4 (FWD_RESPONSE)  | QUOTA_FORWARD_RESPONSE            |
// | 0xC5 (FWD_REJECT)    | QUOTA_FORWARD_REJECT              |
// | 0xC6 (CAP_GOSSIP)    | QUOTA_CAPACITY_GOSSIP             |
// | 0xC7 (CAP_REQUEST)   | QUOTA_CAPACITY_REQUEST            |
// | 0xCA (ROUTER_ANNC)   | QUOTA_ROUTER_ANNOUNCE             |
// | 0xCB (ROUTER_WITHD)  | QUOTA_ROUTER_WITHDRAW             |

/// Router announce (RFC-0870 §NodeEnvelope Adoption).
///
/// UUID: `0x0009:0003:0000:0000:0000:0000:0000:0000`
pub const QUOTA_ROUTER_ANNOUNCE: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
]);
/// Companion `def_line` for `QUOTA_ROUTER_ANNOUNCE` (compiler-resolved via `line!()`).
pub(crate) const __DEF_LINE_QUOTA_ROUTER_ANNOUNCE: u32 = line!();

/// Router withdraw (RFC-0870 §NodeEnvelope Adoption).
///
/// UUID: `0x0009:0003:0000:0000:0000:0000:0000:0001`
pub const QUOTA_ROUTER_WITHDRAW: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
]);
/// Companion `def_line` for `QUOTA_ROUTER_WITHDRAW` (compiler-resolved via `line!()`).
pub(crate) const __DEF_LINE_QUOTA_ROUTER_WITHDRAW: u32 = line!();

/// Capacity gossip (RFC-0870 §NodeEnvelope Adoption).
///
/// UUID: `0x0009:0003:0000:0000:0000:0000:0000:0002`
pub const QUOTA_CAPACITY_GOSSIP: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02,
]);
/// Companion `def_line` for `QUOTA_CAPACITY_GOSSIP` (compiler-resolved via `line!()`).
pub(crate) const __DEF_LINE_QUOTA_CAPACITY_GOSSIP: u32 = line!();

/// Capacity request (RFC-0870 §NodeEnvelope Adoption).
///
/// UUID: `0x0009:0003:0000:0000:0000:0000:0000:0003`
pub const QUOTA_CAPACITY_REQUEST: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03,
]);
/// Companion `def_line` for `QUOTA_CAPACITY_REQUEST` (compiler-resolved via `line!()`).
pub(crate) const __DEF_LINE_QUOTA_CAPACITY_REQUEST: u32 = line!();

/// Forward request (RFC-0870 §NodeEnvelope Adoption).
///
/// UUID: `0x0009:0003:0000:0000:0000:0000:0000:0010`
pub const QUOTA_FORWARD_REQUEST: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10,
]);
/// Companion `def_line` for `QUOTA_FORWARD_REQUEST` (compiler-resolved via `line!()`).
pub(crate) const __DEF_LINE_QUOTA_FORWARD_REQUEST: u32 = line!();

/// Forward response (RFC-0870 §NodeEnvelope Adoption).
///
/// UUID: `0x0009:0003:0000:0000:0000:0000:0000:0011`
pub const QUOTA_FORWARD_RESPONSE: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x11,
]);
/// Companion `def_line` for `QUOTA_FORWARD_RESPONSE` (compiler-resolved via `line!()`).
pub(crate) const __DEF_LINE_QUOTA_FORWARD_RESPONSE: u32 = line!();

/// Forward reject (RFC-0870 §NodeEnvelope Adoption).
///
/// UUID: `0x0009:0003:0000:0000:0000:0000:0000:0012`
pub const QUOTA_FORWARD_REJECT: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x12,
]);
/// Companion `def_line` for `QUOTA_FORWARD_REJECT` (compiler-resolved via `line!()`).
pub(crate) const __DEF_LINE_QUOTA_FORWARD_REJECT: u32 = line!();

/// All RFC-0870 quota-router payload kinds (mission 0870-b-envelope-adoption).
///
/// Mission 0870-b uses this array to perform `on_receive` dispatch: a
/// borsh-deserialized `NodeEnvelope` whose `payload_kind` matches one of
/// these UUIDs is routed to the corresponding `QuotaRouterHandler::handle_*`
/// method. Unknown UUIDs in the RFC-0870 sub-namespace are dropped
/// fail-closed per RFC-0871 §Compatibility + RFC-0965 §3.2.
pub const QUOTA_PAYLOAD_KINDS: &[PayloadKindId] = &[
    QUOTA_ROUTER_ANNOUNCE,
    QUOTA_ROUTER_WITHDRAW,
    QUOTA_CAPACITY_GOSSIP,
    QUOTA_CAPACITY_REQUEST,
    QUOTA_FORWARD_REQUEST,
    QUOTA_FORWARD_RESPONSE,
    QUOTA_FORWARD_REJECT,
];

/// True if `kind` is an RFC-0870 quota-router payload kind.
#[must_use]
pub fn is_quota_payload_kind(kind: &PayloadKindId) -> bool {
    QUOTA_PAYLOAD_KINDS.contains(kind)
}

// RFC-0871 reputation anchor node payload kinds (RFC-0871 §Roles and
// Authorities, mission 0871c-reputation-anchor-node Phase 3). Allocated
// in the RFC-0871 `rfc_namespace` (`0x0009:...`) with sub-namespace
// `0x0004` (mission 0871c — reputation anchor specialized node). The
// full RFC-0968 / RFC-0955-R1 reputation surface (`REPUTATION_QUERY`,
// `REPUTATION_UPDATE`, `REPUTATION_ANCHOR`) lands in follow-on missions
// once the RFC-0968 reputation registry + RFC-0955-R1 anchoring substrate
// are production-ready (mission 0968a-reputation-anchoring in flight).
//
// Mission 0871c AC exposes ONLY the `REPUTATION_ANCHOR_QUERY` adapter
// stub — a typed hand-off point so the L1 quorum flow can route
// reputation-anchor lookups without the registry substrate being live.

/// Reputation anchor query (RFC-0871 §Roles and Authorities, mission 0871c).
///
/// Phase 3 MVP stub: validates a canonical DID via
/// `octo_ident::CanonicalCodec::parse(s, false)` and returns a placeholder
/// `(anchor_score, attestation_count)` response. The real lookup against
/// `octo-reputation` registry lands in mission 0968a-reputation-anchoring.
///
/// UUID: `0x0009:0004:0000:0000:0000:0000:0000:0001`
pub const REPUTATION_ANCHOR_QUERY: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
]);
/// Companion `def_line` for `REPUTATION_ANCHOR_QUERY` (compiler-resolved via `line!()`).
pub(crate) const __DEF_LINE_REPUTATION_ANCHOR_QUERY: u32 = line!();

/// All reputation-anchor payload kinds served by `ReputationAnchorNode`
/// (RFC-0871 §Roles and Authorities, mission 0871c-reputation-anchor-node).
///
/// Phase 3 MVP exposes only `REPUTATION_ANCHOR_QUERY`. Follow-on
/// missions add `REPUTATION_QUERY` / `REPUTATION_UPDATE` / `REPUTATION_ANCHOR`
/// once the RFC-0968 registry + RFC-0955-R1 anchoring substrate are wired.
pub const REPUTATION_PAYLOAD_KINDS: &[PayloadKindId] = &[REPUTATION_ANCHOR_QUERY];

/// True if `kind` is a reputation-anchor payload kind (RFC-0871
/// §Roles and Authorities, mission 0871c).
#[must_use]
pub fn is_reputation_payload_kind(kind: &PayloadKindId) -> bool {
    REPUTATION_PAYLOAD_KINDS.contains(kind)
}

// RFC-0871 capability issuer node payload kinds (RFC-0871 §Roles and
// Authorities, mission 0871d-capability-issuer-node Phase 3). Allocated
// in the RFC-0871 `rfc_namespace` (`0x0009:...`) with sub-namespace
// `0x0005` (mission 0871d — capability issuer specialized node).
//
// Sub-namespace `0x0005` follows the existing pattern: `0x0002` (wallet
// — 0871a), `0x0003` (quota router — 0870-b), `0x0004` (reputation
// anchor — 0871c). Each new specialized node gets its own sub-namespace
// to keep dispatch unambiguous.
//
// Phase 3 MVP exposes `CAPABILITY_ISSUE` + `CAPABILITY_REVOKE`. The
// full RFC-0957 §Algorithms macaroon surface (`CAPABILITY_LOOKUP` +
// `CAPABILITY_ATTENUATE`) lands in follow-on missions once the
// `HolderRegistry` substrate (RFC-0957-A1) is wired in production.
//
// Mission 0871d AC exposes these as **adapter stubs**: typed hand-off
// points that validate canonical DID inputs (issuer's `from_did` +
// holder DID in `CAPABILITY_ISSUE`) and return placeholder wire forms.
// The full macaroon mint + revocation substrate (RFC-0957
// §Algorithms + RFC-0957-A1 §HolderRecord State Machine) lands in
// mission 0957 Phase 2 follow-on (macaroon struct + caveat +
// discharge + wire migrations).

/// Capability issue (RFC-0871 §Roles and Authorities, mission 0871d).
///
/// Phase 3 MVP stub: validates `holder_did` via canonical DID codec and
/// returns a placeholder `CIPHEROCTO_ISSUE_V1:<holder_did>:<token_id>`
/// wire form. The full macaroon mint (`CapabilityToken::mint`,
/// `holder.sign`, `HolderRegistry` registration per RFC-0957 §Algorithms
/// and RFC-0957-A1 §Data Structures) lands in mission 0957 Phase 2
/// follow-on and is plugged in here via the macaroon substrate.
///
/// UUID: `0x0009:0005:0000:0000:0000:0000:0000:0001`
pub const CAPABILITY_ISSUE: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
]);
/// Companion `def_line` for `CAPABILITY_ISSUE` (compiler-resolved via `line!()`).
pub(crate) const __DEF_LINE_CAPABILITY_ISSUE: u32 = line!();

/// Capability revoke (RFC-0871 §Roles and Authorities, mission 0871d).
///
/// Phase 3 MVP stub: validates `token_id` length (16 bytes per
/// RFC-0957 §Wire Format `token_id = macaroon_id`) and returns a
/// placeholder acknowledgement. The full revocation flow
/// (RFC-0957-A1 §HolderRecord State Machine transitions and RFC-0965
/// `RevocationCaveat`) lands in mission 0957 Phase 2 follow-on.
///
/// UUID: `0x0009:0005:0000:0000:0000:0000:0000:0002`
pub const CAPABILITY_REVOKE: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02,
]);
/// Companion `def_line` for `CAPABILITY_REVOKE` (compiler-resolved via `line!()`).
pub(crate) const __DEF_LINE_CAPABILITY_REVOKE: u32 = line!();

/// Capability lookup (RFC-0871 §Roles and Authorities, mission
/// 0957-phase2c). Returns the `HolderRecord` for a given 32-byte
/// `cap_root_hash` PK (RFC-0957-A1 §Data Structures) or `None` if
/// absent.
///
/// Wired in mission 0957-phase2c alongside the macaroon substrate +
/// `HolderRegistry` production-readiness. UUID continues the
/// capability-issuer sub-namespace `0x0005`.
///
/// UUID: `0x0009:0005:0000:0000:0000:0000:0000:0003`
pub const CAPABILITY_LOOKUP: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03,
]);
/// Companion `def_line` for `CAPABILITY_LOOKUP` (compiler-resolved via `line!()`).
pub(crate) const __DEF_LINE_CAPABILITY_LOOKUP: u32 = line!();

/// All capability-issuer payload kinds served by `CapabilityIssuerNode`
/// (RFC-0871 §Roles and Authorities, mission 0871d-capability-issuer-node).
///
/// Phase 3 MVP exposes `CAPABILITY_ISSUE` + `CAPABILITY_REVOKE`. Mission
/// 0957-phase2c adds `CAPABILITY_LOOKUP` once the macaroon substrate
/// (mission 0957 Phase 2) + `HolderRegistry` (RFC-0957-A1) are wired
/// in production. `CAPABILITY_ATTENUATE` lands in a follow-on mission.
pub const CAPABILITY_PAYLOAD_KINDS: &[PayloadKindId] =
    &[CAPABILITY_ISSUE, CAPABILITY_REVOKE, CAPABILITY_LOOKUP];

/// True if `kind` is a capability-issuer payload kind (RFC-0871
/// §Roles and Authorities, mission 0871d).
#[must_use]
pub fn is_capability_payload_kind(kind: &PayloadKindId) -> bool {
    CAPABILITY_PAYLOAD_KINDS.contains(kind)
}

// RFC-0871 paid-query verifier payload kinds (RFC-0871 §Wallet Node
// Lifecycle, mission 0871e-paid-query-caveat Phase 5).
//
// Sub-namespace `0x0006` continues the existing per-specialized-node
// allocation pattern: `0x0001` (identity — 0871), `0x0002` (wallet —
// 0871a), `0x0003` (quota router — 0870-b), `0x0004` (reputation
// anchor — 0871c), `0x0005` (capability issuer — 0871d). Phase 5
// introduces `0x0006` for the paid-query caveat bridge.
//
// Mission 0871e AC exposes ONLY `PAID_QUERY_VERIFY` — a query verifier
// that takes a macaroon token + a `PaidQueryCaveat` (RFC-0965 reserved
// range 0x1A) and returns whether the query is authorized + the
// rate-limit budget. The full RFC-0871 §Implementation Phases Phase 5
// surface (`PAID_QUERY_RECEIPT`, `PAID_QUERY_REFRESH`, etc.) lands in
// follow-on missions once the `RouterAnnouncePayload::pricing_policy`
// extension + atomic drain substrate (RFC-0862) are wired.

/// Paid-query verify (RFC-0871 §Wallet Node Lifecycle, mission 0871e).
///
/// Phase 5 MVP bridge: takes a macaroon `MacaroonId` + a `PaidQueryCaveat`
/// and a `query_cost: u128` (in MicroOCTO_W), returns whether the query
/// is authorized (budget >= cost) + the remaining rate-limit budget.
///
/// The full `PaymentCaveat` composition + atomic drain (RFC-0862) lands
/// in follow-on; this bridge proves the per-extension crate pattern
/// (Layer E) for paid-query variants.
///
/// UUID: `0x0009:0006:0000:0000:0000:0000:0000:0001`
pub const PAID_QUERY_VERIFY: PayloadKindId = PayloadKindId([
    0x00, 0x09, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
]);
/// Companion `def_line` for `PAID_QUERY_VERIFY` (compiler-resolved via `line!()`).
pub(crate) const __DEF_LINE_PAID_QUERY_VERIFY: u32 = line!();

/// All paid-query payload kinds served by the paid-query verifier
/// (RFC-0871 §Wallet Node Lifecycle, mission 0871e-paid-query-caveat).
///
/// Phase 5 MVP exposes `PAID_QUERY_VERIFY`. Follow-on missions add
/// `PAID_QUERY_RECEIPT` + `PAID_QUERY_REFRESH` once the
/// `RouterAnnouncePayload::pricing_policy` extension + atomic drain
/// substrate (RFC-0862) are wired.
pub const PAID_QUERY_PAYLOAD_KINDS: &[PayloadKindId] = &[PAID_QUERY_VERIFY];

/// True if `kind` is a paid-query payload kind (RFC-0871
/// §Wallet Node Lifecycle, mission 0871e).
#[must_use]
pub fn is_paid_query_payload_kind(kind: &PayloadKindId) -> bool {
    PAID_QUERY_PAYLOAD_KINDS.contains(kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_resolve_uuid_matches_tv1() {
        // RFC-0871 §TV1: payload_kind = UUID 0x0009:0001:0000:0000:0000:0000:0000:0001
        let expected: [u8; 16] = [
            0x00, 0x09, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x01,
        ];
        assert_eq!(IDENTITY_RESOLVE.0, expected);
    }

    #[test]
    fn identity_register_uuid_matches_rfc_0862_v13() {
        // Mission 0871e-f7-impl-resolver-mediation:
        // IDENTITY_REGISTER = UUID 0x0009:0001:0000:0000:0000:0000:0000:0002
        let expected: [u8; 16] = [
            0x00, 0x09, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x02,
        ];
        assert_eq!(IDENTITY_REGISTER.0, expected);
    }

    #[test]
    fn identity_revoke_uuid_matches_rfc_0862_v13() {
        // Mission 0871e-f7-impl-resolver-mediation:
        // IDENTITY_REVOKE = UUID 0x0009:0001:0000:0000:0000:0000:0000:0003
        let expected: [u8; 16] = [
            0x00, 0x09, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x03,
        ];
        assert_eq!(IDENTITY_REVOKE.0, expected);
    }

    #[test]
    fn identity_payload_kinds_are_distinct() {
        // Mission 0871e-f7-impl-resolver-mediation AC: 6 identity
        // payload kinds must be pairwise distinct (no accidental UUID
        // collision across `IDENTITY_RESOLVE` / `IDENTITY_REGISTER` /
        // `IDENTITY_REVOKE` / `IDENTITY_RESOLVE_CHAIN` /
        // `IDENTITY_RESOLVE_WITH_CHAIN` /
        // `IDENTITY_RESOLVE_CHAIN_RESPONSE`).
        // Mission 0871b-cross-domain-resolution-impl adds `IDENTITY_RESOLVE_CHAIN`
        // (UUID slot 0004 in identity sub-namespace 0x0009:0001:...).
        // Mission 0010-f2-multi-chain-routing adds
        // `IDENTITY_RESOLVE_WITH_CHAIN` (UUID slot 0005).
        // Mission 0870k-transport-request-response AC-6 adds
        // `IDENTITY_RESOLVE_CHAIN_RESPONSE` (UUID slot 0006 —
        // cross-network chain reply payload kind).
        let kinds = [
            IDENTITY_RESOLVE,
            IDENTITY_REGISTER,
            IDENTITY_REVOKE,
            IDENTITY_RESOLVE_CHAIN,
            IDENTITY_RESOLVE_WITH_CHAIN,
            IDENTITY_RESOLVE_CHAIN_RESPONSE,
        ];
        assert_eq!(kinds.len(), 6);
        for i in 0..kinds.len() {
            for j in (i + 1)..kinds.len() {
                assert_ne!(
                    kinds[i], kinds[j],
                    "duplicate UUID in identity payload kinds at indices {i} / {j}"
                );
            }
        }
        // Also assert they do NOT collide with the existing wallet kinds
        // (different sub-namespace, but a defensive cross-check guards
        // against future re-allocations).
        assert_ne!(IDENTITY_REGISTER, WALLET_SIGN_ED25519);
        assert_ne!(IDENTITY_REVOKE, WALLET_RESOLVE_DID);
        assert_ne!(IDENTITY_RESOLVE_CHAIN, WALLET_RESOLVE_DID);
        assert_ne!(IDENTITY_RESOLVE_CHAIN_RESPONSE, WALLET_RESOLVE_DID);
    }

    #[test]
    fn identity_resolve_chain_uuid_matches_mission_0871b() {
        // Mission 0871b-cross-domain-resolution-impl AC:
        // IDENTITY_RESOLVE_CHAIN = UUID 0x0009:0001:0000:0000:0000:0000:0000:0004
        // (next free slot in identity sub-namespace after RESOLVE/REGISTER/REVOKE).
        let expected: [u8; 16] = [
            0x00, 0x09, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x04,
        ];
        assert_eq!(IDENTITY_RESOLVE_CHAIN.0, expected);
    }

    #[test]
    fn identity_resolve_chain_is_rfc_allocated() {
        // Mission 0871b AC: IDENTITY_RESOLVE_CHAIN must sit in the
        // RFC-0871 `rfc_namespace` (0x0009:0000…0x0009:FFFF) with
        // sub-namespace 0x0001 (mission 0871 — identity).
        assert!(IDENTITY_RESOLVE_CHAIN.is_rfc_allocated());
        assert_eq!(IDENTITY_RESOLVE_CHAIN.0[0], 0x00);
        assert_eq!(IDENTITY_RESOLVE_CHAIN.0[1], 0x09);
        assert_eq!(IDENTITY_RESOLVE_CHAIN.0[2], 0x00);
        assert_eq!(IDENTITY_RESOLVE_CHAIN.0[3], 0x01);
    }

    #[test]
    fn identity_resolve_chain_borsh_round_trip() {
        let bytes = borsh::to_vec(&IDENTITY_RESOLVE_CHAIN).unwrap();
        let back: PayloadKindId = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, IDENTITY_RESOLVE_CHAIN);
    }

    #[test]
    fn rfc_namespace_classification() {
        assert!(IDENTITY_RESOLVE.is_rfc_allocated());
        assert!(!IDENTITY_RESOLVE.is_capability_extension());
        assert!(!IDENTITY_RESOLVE.is_user_extension());
    }

    #[test]
    fn user_extension_namespace_high_bytes() {
        let ext = PayloadKindId([0xFF; 16]);
        assert!(ext.is_user_extension());
        assert!(!ext.is_rfc_allocated());
    }

    #[test]
    fn borsh_round_trip() {
        let bytes = borsh::to_vec(&IDENTITY_RESOLVE).unwrap();
        let back: PayloadKindId = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, IDENTITY_RESOLVE);
    }

    #[test]
    fn quota_payload_kinds_are_distinct() {
        // Mission 0870-b AC: 7 RFC-0870 payload kinds must be pairwise distinct
        // (no two payloads share a UUID by accident).
        let kinds = QUOTA_PAYLOAD_KINDS;
        assert_eq!(kinds.len(), 7);
        for i in 0..kinds.len() {
            for j in (i + 1)..kinds.len() {
                assert_ne!(
                    kinds[i], kinds[j],
                    "duplicate UUID in QUOTA_PAYLOAD_KINDS at indices {i} / {j}"
                );
            }
        }
    }

    #[test]
    fn quota_payload_kinds_are_rfc_allocated() {
        // Mission 0870-b AC: every RFC-0870 payload kind MUST sit in the
        // RFC-0871 `rfc_namespace` (0x0009:0000…0x0009:FFFF). Legacy
        // outbound code reads the first byte to discriminate legacy vs new
        // envelopes; new envelopes MUST be borsh-decodable as `NodeEnvelope`.
        for kind in QUOTA_PAYLOAD_KINDS {
            assert!(
                kind.is_rfc_allocated(),
                "RFC-0870 payload kind {kind:?} not in rfc_namespace"
            );
        }
    }

    #[test]
    fn quota_payload_kinds_first16bits_0x0009_0003() {
        // Mission 0870-b AC: RFC-0870 sub-namespace = 0x0003 (within
        // RFC-0871 rfc_namespace). First 16 bits must be 0x0009; next 16
        // bits must be 0x0003.
        for kind in QUOTA_PAYLOAD_KINDS {
            assert_eq!(kind.0[0], 0x00);
            assert_eq!(kind.0[1], 0x09);
            assert_eq!(kind.0[2], 0x00);
            assert_eq!(kind.0[3], 0x03);
        }
    }

    #[test]
    fn is_quota_payload_kind_matches_array() {
        for kind in QUOTA_PAYLOAD_KINDS {
            assert!(is_quota_payload_kind(kind));
        }
        // A non-quota RFC-0871 payload kind must NOT match.
        assert!(!is_quota_payload_kind(&IDENTITY_RESOLVE));
        assert!(!is_quota_payload_kind(&WALLET_SIGN_ED25519));
    }

    #[test]
    fn reputation_anchor_query_uuid_matches_mission_0871c() {
        // Mission 0871c AC: REPUTATION_ANCHOR_QUERY UUID =
        // 0x0009:0004:0000:0000:0000:0000:0000:0001
        let expected: [u8; 16] = [
            0x00, 0x09, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x01,
        ];
        assert_eq!(REPUTATION_ANCHOR_QUERY.0, expected);
    }

    #[test]
    fn reputation_payload_kinds_are_rfc_allocated() {
        // Mission 0871c AC: every reputation-anchor payload kind MUST sit in
        // the RFC-0871 `rfc_namespace` (0x0009:0000…0x0009:FFFF) with
        // sub-namespace `0x0004` (mission 0871c — reputation anchor).
        for kind in REPUTATION_PAYLOAD_KINDS {
            assert!(
                kind.is_rfc_allocated(),
                "RFC-0871 reputation payload kind {kind:?} not in rfc_namespace"
            );
            assert_eq!(kind.0[0], 0x00);
            assert_eq!(kind.0[1], 0x09);
            assert_eq!(kind.0[2], 0x00);
            assert_eq!(kind.0[3], 0x04);
        }
    }

    #[test]
    fn is_reputation_payload_kind_matches_array() {
        for kind in REPUTATION_PAYLOAD_KINDS {
            assert!(is_reputation_payload_kind(kind));
        }
        // A non-reputation RFC-0871 payload kind must NOT match.
        assert!(!is_reputation_payload_kind(&IDENTITY_RESOLVE));
        assert!(!is_reputation_payload_kind(&WALLET_SIGN_ED25519));
        assert!(!is_reputation_payload_kind(&QUOTA_ROUTER_ANNOUNCE));
    }

    #[test]
    fn reputation_payload_kinds_borsh_round_trip() {
        for kind in REPUTATION_PAYLOAD_KINDS {
            let bytes = borsh::to_vec(kind).unwrap();
            let back: PayloadKindId = borsh::from_slice(&bytes).unwrap();
            assert_eq!(back, *kind);
        }
    }

    #[test]
    fn reputation_anchor_query_does_not_collide_with_quota() {
        // Mission 0871c AC: REPUTATION_ANCHOR_QUERY must NOT collide with
        // the RFC-0870 quota-router sub-namespace (0x0003). The dispatcher
        // classifies by exact UUID match; a collision would silently route
        // reputation lookups to the quota-router receiver.
        assert!(!is_quota_payload_kind(&REPUTATION_ANCHOR_QUERY));
        assert!(!is_reputation_payload_kind(&QUOTA_ROUTER_ANNOUNCE));
        assert_ne!(REPUTATION_ANCHOR_QUERY, QUOTA_ROUTER_ANNOUNCE);
        assert_ne!(REPUTATION_ANCHOR_QUERY, WALLET_RESOLVE_DID);
    }

    #[test]
    fn quota_payload_kinds_borsh_round_trip() {
        for kind in QUOTA_PAYLOAD_KINDS {
            let bytes = borsh::to_vec(kind).unwrap();
            let back: PayloadKindId = borsh::from_slice(&bytes).unwrap();
            assert_eq!(back, *kind);
        }
    }

    #[test]
    fn capability_issue_uuid_matches_mission_0871d() {
        // Mission 0871d AC: CAPABILITY_ISSUE UUID =
        // 0x0009:0005:0000:0000:0000:0000:0000:0001
        let expected: [u8; 16] = [
            0x00, 0x09, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x01,
        ];
        assert_eq!(CAPABILITY_ISSUE.0, expected);
    }

    #[test]
    fn capability_revoke_uuid_matches_mission_0871d() {
        // Mission 0871d AC: CAPABILITY_REVOKE UUID =
        // 0x0009:0005:0000:0000:0000:0000:0000:0002
        let expected: [u8; 16] = [
            0x00, 0x09, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x02,
        ];
        assert_eq!(CAPABILITY_REVOKE.0, expected);
    }

    #[test]
    fn capability_payload_kinds_are_distinct() {
        // Mission 0871d AC: capability-issuer payload kinds must be
        // pairwise distinct (no two payloads share a UUID by accident).
        // 0957-phase2c added `CAPABILITY_LOOKUP`; this assertion
        // generalizes to an N-way distinctness check via `BTreeSet`.
        let kinds = CAPABILITY_PAYLOAD_KINDS;
        assert!(
            kinds.len() >= 2,
            "expected at least 2 capability payload kinds, got {}",
            kinds.len()
        );
        let unique: std::collections::HashSet<_> = kinds.iter().collect();
        assert_eq!(
            unique.len(),
            kinds.len(),
            "capability payload kinds must be pairwise distinct"
        );
    }

    #[test]
    fn capability_payload_kinds_are_rfc_allocated() {
        // Mission 0871d AC: every capability-issuer payload kind MUST sit
        // in the RFC-0871 `rfc_namespace` (0x0009:0000…0x0009:FFFF) with
        // sub-namespace `0x0005` (mission 0871d — capability issuer).
        for kind in CAPABILITY_PAYLOAD_KINDS {
            assert!(
                kind.is_rfc_allocated(),
                "RFC-0871 capability payload kind {kind:?} not in rfc_namespace"
            );
            assert_eq!(kind.0[0], 0x00);
            assert_eq!(kind.0[1], 0x09);
            assert_eq!(kind.0[2], 0x00);
            assert_eq!(kind.0[3], 0x05);
        }
    }

    #[test]
    fn capability_payload_kinds_first16bits_0x0009_0005() {
        // Mission 0871d AC: capability-issuer sub-namespace = 0x0005
        // (within RFC-0871 rfc_namespace). First 16 bits must be 0x0009;
        // next 16 bits must be 0x0005.
        for kind in CAPABILITY_PAYLOAD_KINDS {
            assert_eq!(kind.0[0], 0x00);
            assert_eq!(kind.0[1], 0x09);
            assert_eq!(kind.0[2], 0x00);
            assert_eq!(kind.0[3], 0x05);
        }
    }

    #[test]
    fn is_capability_payload_kind_matches_array() {
        for kind in CAPABILITY_PAYLOAD_KINDS {
            assert!(is_capability_payload_kind(kind));
        }
        // Non-capability RFC-0871 payload kinds must NOT match.
        assert!(!is_capability_payload_kind(&IDENTITY_RESOLVE));
        assert!(!is_capability_payload_kind(&WALLET_SIGN_ED25519));
        assert!(!is_capability_payload_kind(&QUOTA_ROUTER_ANNOUNCE));
        assert!(!is_capability_payload_kind(&REPUTATION_ANCHOR_QUERY));
    }

    #[test]
    fn capability_payload_kinds_borsh_round_trip() {
        for kind in CAPABILITY_PAYLOAD_KINDS {
            let bytes = borsh::to_vec(kind).unwrap();
            let back: PayloadKindId = borsh::from_slice(&bytes).unwrap();
            assert_eq!(back, *kind);
        }
    }

    #[test]
    fn capability_issue_does_not_collide_with_wallet_or_reputation() {
        // Mission 0871d AC: CAPABILITY_ISSUE must NOT collide with the
        // wallet (0x0002), quota router (0x0003), or reputation anchor
        // (0x0004) sub-namespaces. The dispatcher classifies by exact
        // UUID match; a collision would silently route capability
        // issuance to the wrong receiver.
        assert!(!is_wallet_payload_kind_placeholder(&CAPABILITY_ISSUE));
        assert!(!is_reputation_payload_kind(&CAPABILITY_ISSUE));
        assert!(!is_quota_payload_kind(&CAPABILITY_ISSUE));
        assert_ne!(CAPABILITY_ISSUE, WALLET_MINT_CAPABILITY);
        assert_ne!(CAPABILITY_ISSUE, REPUTATION_ANCHOR_QUERY);
        assert_ne!(CAPABILITY_ISSUE, QUOTA_ROUTER_ANNOUNCE);
    }

    #[test]
    fn capability_revoke_does_not_collide_with_wallet_or_reputation() {
        // Mission 0871d AC: CAPABILITY_REVOKE must NOT collide with the
        // wallet, quota router, or reputation anchor sub-namespaces.
        assert!(!is_wallet_payload_kind_placeholder(&CAPABILITY_REVOKE));
        assert!(!is_reputation_payload_kind(&CAPABILITY_REVOKE));
        assert!(!is_quota_payload_kind(&CAPABILITY_REVOKE));
        assert_ne!(CAPABILITY_REVOKE, WALLET_ATTENUATE_CAPABILITY);
        assert_ne!(CAPABILITY_REVOKE, REPUTATION_ANCHOR_QUERY);
        assert_ne!(CAPABILITY_REVOKE, QUOTA_ROUTER_ANNOUNCE);
    }

    // Local helper — wallet-kind test predicate lives in `octo_wallet_node`
    // crate; we expose a minimal local test predicate here so the cross-
    // namespace collision test doesn't take a transitive crate dep just
    // for one assertion.
    fn is_wallet_payload_kind_placeholder(kind: &PayloadKindId) -> bool {
        kind.0[0] == 0x00 && kind.0[1] == 0x09 && kind.0[2] == 0x00 && kind.0[3] == 0x02
    }

    // ── Mission 0871e-paid-query-caveat (Phase 5) AC tests ──

    #[test]
    fn paid_query_verify_uuid_matches_mission_0871e() {
        // Mission 0871e AC: PAID_QUERY_VERIFY UUID =
        // 0x0009:0006:0000:0000:0000:0000:0000:0001
        let expected: [u8; 16] = [
            0x00, 0x09, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x01,
        ];
        assert_eq!(PAID_QUERY_VERIFY.0, expected);
    }

    #[test]
    fn paid_query_payload_kinds_are_rfc_allocated() {
        // Mission 0871e AC: every paid-query payload kind MUST sit in the
        // RFC-0871 `rfc_namespace` (0x0009:0000…0x0009:FFFF) with
        // sub-namespace `0x0006` (mission 0871e — paid query).
        for kind in PAID_QUERY_PAYLOAD_KINDS {
            assert!(
                kind.is_rfc_allocated(),
                "RFC-0871 paid-query payload kind {kind:?} not in rfc_namespace"
            );
            assert_eq!(kind.0[0], 0x00);
            assert_eq!(kind.0[1], 0x09);
            assert_eq!(kind.0[2], 0x00);
            assert_eq!(kind.0[3], 0x06);
        }
    }

    #[test]
    fn is_paid_query_payload_kind_matches_array() {
        for kind in PAID_QUERY_PAYLOAD_KINDS {
            assert!(is_paid_query_payload_kind(kind));
        }
        // Non-paid-query RFC-0871 payload kinds must NOT match.
        assert!(!is_paid_query_payload_kind(&IDENTITY_RESOLVE));
        assert!(!is_paid_query_payload_kind(&WALLET_SIGN_ED25519));
        assert!(!is_paid_query_payload_kind(&QUOTA_ROUTER_ANNOUNCE));
        assert!(!is_paid_query_payload_kind(&REPUTATION_ANCHOR_QUERY));
        assert!(!is_paid_query_payload_kind(&CAPABILITY_ISSUE));
    }

    #[test]
    fn paid_query_payload_kinds_borsh_round_trip() {
        for kind in PAID_QUERY_PAYLOAD_KINDS {
            let bytes = borsh::to_vec(kind).unwrap();
            let back: PayloadKindId = borsh::from_slice(&bytes).unwrap();
            assert_eq!(back, *kind);
        }
    }

    #[test]
    fn paid_query_verify_does_not_collide_with_other_namespaces() {
        // Mission 0871e AC: PAID_QUERY_VERIFY must NOT collide with
        // wallet (0x0002), quota router (0x0003), reputation anchor
        // (0x0004), or capability issuer (0x0005) sub-namespaces.
        // The dispatcher classifies by exact UUID match; a collision
        // would silently route paid-query verification to the wrong
        // receiver.
        assert!(!is_wallet_payload_kind_placeholder(&PAID_QUERY_VERIFY));
        assert!(!is_quota_payload_kind(&PAID_QUERY_VERIFY));
        assert!(!is_reputation_payload_kind(&PAID_QUERY_VERIFY));
        assert!(!is_capability_payload_kind(&PAID_QUERY_VERIFY));
        assert_ne!(PAID_QUERY_VERIFY, WALLET_MINT_CAPABILITY);
        assert_ne!(PAID_QUERY_VERIFY, QUOTA_ROUTER_ANNOUNCE);
        assert_ne!(PAID_QUERY_VERIFY, REPUTATION_ANCHOR_QUERY);
        assert_ne!(PAID_QUERY_VERIFY, CAPABILITY_ISSUE);
    }
}
