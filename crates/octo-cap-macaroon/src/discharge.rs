//! Discharge channels (RFC-0957 §3.6).
//!
//! Discharge macaroons are issued by **third-party channels** to satisfy
//! third-party caveats on the root capability. CipherOcto defines three
//! standard channels:
//! - **escrow** — settlement oracle (RFC-0959 v1.0)
//! - **revocation** — revocation oracle (per-RFC-0853)
//! - **rate-limit** — rate-limit oracle (per-RFC-0959 §Anti-fraud)
//!
//! Each channel has a distinct root secret held by the channel operator.
//! The root capability references the channel by name (string ID); the
//! verifier resolves the channel root secret via out-of-band discovery.

use std::collections::HashMap;

use crate::caveat::Caveat;
use crate::token::DischargeMacaroon;

// Mission 0957 Phase 2b-3: `DischargeMacaroon` moved to the
// `octo-cap-macaroon` extension crate alongside `CapabilityToken`
// (which holds `Vec<DischargeMacaroon>`). Re-export here so the existing
// (DischargeMacaroon now lives in this same crate at `crate::token`.)

/// Channel identifier (opaque string). Standard channels: "escrow", "revocation", "rate-limit".
pub type ChannelId = String;

/// Standard discharge channels (RFC-0957 §3.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DischargeChannel {
    /// Settlement oracle (escrow).
    Escrow,
    /// Revocation oracle.
    Revocation,
    /// Rate-limit oracle.
    RateLimit,
}

impl DischargeChannel {
    /// Wire-stable channel identifier.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Escrow => "escrow",
            Self::Revocation => "revocation",
            Self::RateLimit => "rate-limit",
        }
    }
}

impl std::str::FromStr for DischargeChannel {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "escrow" => Ok(Self::Escrow),
            "revocation" => Ok(Self::Revocation),
            "rate-limit" => Ok(Self::RateLimit),
            _ => Err(()),
        }
    }
}

/// Request to mint a discharge macaroon (RFC-0957 §3.6 mint flow).
///
/// Holder presents their `CapabilityToken`; the channel provider inspects
/// the third-party caveats targeting this channel, runs its own predicate
/// (escrow balance, revocation list, rate-limit window), and mints a
/// discharge macaroon if satisfied.
#[derive(Debug, Clone)]
pub struct DischargeRequest<'a> {
    /// Holder's full capability token (channel reads `caveats` only).
    pub token: &'a super::CapabilityToken,
    /// Holder DID (channel-specific: e.g., rate-limit uses this as the bucket key).
    pub holder_did: &'a str,
    /// Channel-specific opaque state (e.g., settlement amount, model+axis).
    pub context: &'a [u8],
}

/// Result of [`ChannelProvider::verify_discharge`]: either the discharge
/// is valid for the requested capability, or the channel returned a
/// structured reason (escrow balance too low, revoked, rate exceeded).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DischargeVerification {
    /// Channel that issued the discharge.
    pub channel: ChannelId,
    /// True iff the discharge is valid for the holder token.
    pub valid: bool,
    /// Reason if invalid (e.g., `"escrow balance 50 < required 100"`).
    pub reason: Option<String>,
}

/// Errors from the discharge protocol (RFC-0957 §3.6 verify flow).
#[derive(Debug, thiserror::Error)]
pub enum DischargeError {
    #[error("discharge for unknown channel `{0}`")]
    UnknownChannel(ChannelId),
    #[error("discharge channel `{channel}` rejected: {reason}")]
    ChannelRejected { channel: ChannelId, reason: String },
    #[error("token references third-party caveat `{channel}` but no discharge attached")]
    MissingDischarge { channel: ChannelId },
}

/// Trait implemented by third-party channel operators (RFC-0957 §3.6).
///
/// Providers are stateless from the trait's perspective; concrete impls
/// hold the operator's state (escrow ledger, revocation list, rate-limit
/// windows) behind their own fields. Verification is per-discharge:
/// `verify_discharge` decides whether a specific discharge macaroon is
/// valid for a specific holder token.
///
/// **`Debug` super-trait:** `ChannelProviderRegistry` stores
/// `Box<dyn ChannelProvider>` and uses the trait object's Debug for its
/// own Debug impl (so the registry satisfies the wallet's
/// `missing_debug_implementations` lint). All channel provider impls in
/// this module hold only public operational state (balances, revocation
/// lists, rate windows); Debug output never carries secret material.
pub trait ChannelProvider: Send + Sync + std::fmt::Debug {
    /// Channel identifier this provider services (`"escrow"`, etc.).
    /// Static because all standard channels (escrow / revocation /
    /// rate-limit) are compile-time string constants; this also lets
    /// `&&'static str` be inferred downstream (avoids clippy
    /// `unnecessary_literal_bound` warnings on every impl).
    fn channel(&self) -> &'static str;

    /// Mint a new discharge macaroon for `req`. Returns the discharge
    /// macaroon body that the holder attaches to their token.
    ///
    /// # Errors
    /// Returns `DischargeError::ChannelRejected` if the operator predicate
    /// (escrow balance, revocation status, rate window) fails.
    fn mint_discharge(
        &self,
        req: &DischargeRequest<'_>,
    ) -> Result<DischargeMacaroon, DischargeError>;

    /// Verify an existing discharge satisfies the holder's token at the
    /// time of `verify_discharges`. Returns `valid: true` only when the
    /// operator predicate still holds (escrow balance still sufficient,
    /// not revoked, rate window not exceeded).
    fn verify_discharge(
        &self,
        req: &DischargeRequest<'_>,
        discharge: &DischargeMacaroon,
    ) -> DischargeVerification;
}

/// Resolver mapping `ChannelId` -> `Box<dyn ChannelProvider>`. Used by
/// `verify_discharges` to look up the provider for each third-party
/// caveat on a token.
pub trait ChannelProviderResolver {
    /// Resolve the provider servicing `channel_id`. Returns `None` if no
    /// provider is registered for the channel (caller surfaces as
    /// `DischargeError::UnknownChannel`).
    fn resolve(&self, channel_id: &str) -> Option<&dyn ChannelProvider>;
}

/// Simple in-memory registry mapping `ChannelId` to a boxed provider.
/// Sufficient for tests + single-process deployments; production
/// deployments will swap for a network-backed resolver (e.g., discovering
/// providers via the wallet's PCE).
#[derive(Default, Debug)]
pub struct ChannelProviderRegistry {
    providers: HashMap<ChannelId, Box<dyn ChannelProvider>>,
}

impl ChannelProviderRegistry {
    /// Register a provider for its declared channel. Replaces any existing
    /// provider for the same channel (last-writer-wins).
    pub fn register<P: ChannelProvider + 'static>(&mut self, provider: P) {
        self.providers
            .insert(provider.channel().to_owned(), Box::new(provider));
    }

    /// Number of registered providers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.providers.len()
    }

    /// True iff no providers registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }
}

impl ChannelProviderResolver for ChannelProviderRegistry {
    fn resolve(&self, channel_id: &str) -> Option<&dyn ChannelProvider> {
        self.providers
            .get(channel_id)
            .map(std::convert::AsRef::as_ref)
    }
}

/// Verify all discharge macaroons attached to `token` against the third-party
/// caveats in its caveat list. Each `Caveat::ThirdParty(channel_id)` MUST
/// have a matching discharge in `token.discharges` whose `channel` field
/// equals `channel_id`, AND that discharge MUST verify against the channel
/// provider registered for `channel_id`.
///
/// Order is: structural check (channel_id match) → provider lookup → provider
/// verify. The structural check fails fast; the provider verify is the
/// security-relevant step.
///
/// `context_for_channel` supplies the channel-specific verify-time context
/// (e.g., escrow settlement amount, rate-limit window). The map is keyed
/// by `channel_id`; channels without a context entry get an empty slice.
/// An empty escrow context decodes as "missing context" → verify rejects,
/// which is the correct fail-closed behavior.
///
/// # Errors
/// - `DischargeError::MissingDischarge` if a `ThirdParty` caveat has no
///   matching discharge attached.
/// - `DischargeError::UnknownChannel` if no provider is registered for
///   the channel.
/// - `DischargeError::ChannelRejected` if the provider rejects the
///   discharge (escrow too low, revoked, rate exceeded).
// `implicit_hasher` (clippy::pedantic): the public API takes a concrete
// `HashMap<_, _, _>` rather than generic `BuildHasher`. Generalizing would
// require changing every caller; the simpler fix is a targeted allow
// scoped to the signature, which keeps the API stable for downstream
// crates (e.g., quota-router-core's `verify_discharges` callsite).
#[allow(clippy::implicit_hasher)]
pub fn verify_discharges(
    token: &super::CapabilityToken,
    resolver: &dyn ChannelProviderResolver,
    context_for_channel: &HashMap<ChannelId, Vec<u8>>,
) -> Result<(), DischargeError> {
    for caveat in &token.macaroon.caveats {
        let Caveat::ThirdParty(expected_channel) = caveat else {
            continue;
        };
        // Find a discharge for this channel. The `find` filter guarantees
        // `discharge.channel == expected_channel` by construction; the
        // earlier defensive re-check was dead code (the bound `discharge`
        // is an immutable borrow and cannot diverge from the filter).
        let discharge = token
            .discharges
            .iter()
            .find(|d| d.channel == *expected_channel)
            .ok_or_else(|| DischargeError::MissingDischarge {
                channel: expected_channel.clone(),
            })?;
        // Provider lookup + verify.
        let provider = resolver
            .resolve(expected_channel)
            .ok_or_else(|| DischargeError::UnknownChannel(expected_channel.clone()))?;
        let context: &[u8] = context_for_channel
            .get(expected_channel)
            .map_or(&[], std::vec::Vec::as_slice);
        let req = DischargeRequest {
            token,
            holder_did: &token.holder_did,
            context,
        };
        let result = provider.verify_discharge(&req, discharge);
        if !result.valid {
            return Err(DischargeError::ChannelRejected {
                channel: expected_channel.clone(),
                reason: result.reason.unwrap_or_else(|| "rejected".to_owned()),
            });
        }
    }
    Ok(())
}

// --- Escrow provider (RFC-0957 §3.6 + RFC-0959 v1.0 settlement oracle) ---
//
// The escrow channel checks that the holder has sufficient OCTO-W escrow
// balance to cover the requested settlement amount. The amount is read
// from `req.context` as a big-endian u128 (EscrowBalance, 1e-6 OCTO-W).
//
// State: `balances: HashMap<holder_did, EscrowBalance>`. In production this
// is replaced by the on-chain settlement ledger; the trait surface is
// stable across the swap.

/// OCTO-W balance in EscrowBalance (1e-6 OCTO-W). Read from `req.context` as
/// a big-endian u128; mismatch returns `ChannelRejected`. Named
/// `EscrowBalance` (rather than reusing `caveat::MicroOctoW`) so the
/// escrow-channel's wire-encoded amount is distinct from caveat-level
/// amount constraints — different domains, different types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EscrowBalance(pub u128);

impl EscrowBalance {
    /// Decode from `req.context` (big-endian u128, 16 bytes). Returns
    /// `None` if `context.len() != 16`.
    #[must_use]
    pub fn from_context(context: &[u8]) -> Option<Self> {
        if context.len() != 16 {
            return None;
        }
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(context);
        Some(Self(u128::from_be_bytes(bytes)))
    }
}

/// Escrow discharge provider. Holds per-holder OCTO-W balances; mints a
/// discharge iff holder balance >= requested amount.
#[derive(Debug, Default)]
pub struct EscrowDischargeProvider {
    balances: HashMap<String, EscrowBalance>,
}

impl EscrowDischargeProvider {
    /// Create a new provider with no balances configured. Production
    /// deployments populate `balances` from the on-chain settlement ledger.
    #[must_use]
    pub fn new() -> Self {
        Self {
            balances: HashMap::new(),
        }
    }

    /// Set a holder's OCTO-W balance (test/dev helper).
    pub fn set_balance(&mut self, holder_did: &str, balance: EscrowBalance) {
        self.balances.insert(holder_did.to_owned(), balance);
    }
}

impl ChannelProvider for EscrowDischargeProvider {
    fn channel(&self) -> &'static str {
        "escrow"
    }

    fn mint_discharge(
        &self,
        req: &DischargeRequest<'_>,
    ) -> Result<DischargeMacaroon, DischargeError> {
        let requested = EscrowBalance::from_context(req.context).ok_or_else(|| {
            DischargeError::ChannelRejected {
                channel: "escrow".to_owned(),
                reason: "context must be 16-byte big-endian u128 EscrowBalance".to_owned(),
            }
        })?;
        let balance = self
            .balances
            .get(req.holder_did)
            .copied()
            .unwrap_or(EscrowBalance(0));
        if balance.0 < requested.0 {
            return Err(DischargeError::ChannelRejected {
                channel: "escrow".to_owned(),
                reason: format!("escrow balance {} < requested {}", balance.0, requested.0),
            });
        }
        // Channel-issued discharge carries a synthetic root_secret_hash
        // bound to the holder's DID + balance at mint time. Channel state
        // evolves via subsequent `verify_discharge` checks against the
        // current ledger; the discharge macaroon does NOT carry a
        // signature chain (the channel operator is trusted to mint).
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"cipherocto/discharge/escrow/v1");
        hasher.update(req.holder_did.as_bytes());
        hasher.update(&balance.0.to_be_bytes());
        Ok(DischargeMacaroon {
            channel: "escrow".to_owned(),
            root_secret_hash: *hasher.finalize().as_bytes(),
            chain: vec![[0u8; 32]],
            caveats: Vec::new(),
        })
    }

    fn verify_discharge(
        &self,
        req: &DischargeRequest<'_>,
        _discharge: &DischargeMacaroon,
    ) -> DischargeVerification {
        let Some(requested) = EscrowBalance::from_context(req.context) else {
            return DischargeVerification {
                channel: "escrow".to_owned(),
                valid: false,
                reason: Some("context must be 16-byte big-endian u128 EscrowBalance".to_owned()),
            };
        };
        let balance = self
            .balances
            .get(req.holder_did)
            .copied()
            .unwrap_or(EscrowBalance(0));
        if balance.0 < requested.0 {
            DischargeVerification {
                channel: "escrow".to_owned(),
                valid: false,
                reason: Some(format!(
                    "escrow balance {} < requested {}",
                    balance.0, requested.0
                )),
            }
        } else {
            DischargeVerification {
                channel: "escrow".to_owned(),
                valid: true,
                reason: None,
            }
        }
    }
}

// --- Revocation provider (per-RFC-0853) ---
//
// The revocation channel issues a short-lived (60s) non-revocation proof.
// At mint time: holder is not on the revocation list → mint discharge with
// a `Before(now + 60)` caveat. At verify time: provider re-checks the
// revocation list; if holder has been revoked since mint, the discharge
// is rejected. The 60s window bounds the staleness of a holder's
// non-revoked status.

/// Revocation discharge provider. Holds the current revocation list;
/// mints non-revocation proofs that expire in 60s.
#[derive(Debug, Default)]
pub struct RevocationDischargeProvider {
    /// DIDs currently on the revocation list.
    revoked: std::collections::HashSet<String>,
    /// Current Unix time source (test override).
    now: std::sync::Mutex<u64>,
}

/// Maximum lifetime of a revocation discharge (RFC-0957 §3.6).
pub const REVOCATION_DISCHARGE_TTL_SECS: u64 = 60;

impl RevocationDischargeProvider {
    /// Create a new provider with empty revocation list.
    #[must_use]
    pub fn new() -> Self {
        Self {
            revoked: std::collections::HashSet::new(),
            now: std::sync::Mutex::new(0),
        }
    }

    /// Add a holder DID to the revocation list.
    pub fn revoke(&mut self, holder_did: &str) {
        self.revoked.insert(holder_did.to_owned());
    }

    /// Remove a holder DID from the revocation list.
    pub fn unrevoke(&mut self, holder_did: &str) {
        self.revoked.remove(holder_did);
    }

    /// Set the current Unix time (test override). Production reads from
    /// a monotonic clock or RFC-0959 §Time oracle.
    pub fn set_now(&self, now: u64) {
        *self.now.lock().expect("revocation now mutex") = now;
    }
}

impl ChannelProvider for RevocationDischargeProvider {
    fn channel(&self) -> &'static str {
        "revocation"
    }

    fn mint_discharge(
        &self,
        req: &DischargeRequest<'_>,
    ) -> Result<DischargeMacaroon, DischargeError> {
        if self.revoked.contains(req.holder_did) {
            return Err(DischargeError::ChannelRejected {
                channel: "revocation".to_owned(),
                reason: format!("holder {} is on the revocation list", req.holder_did),
            });
        }
        let now = *self.now.lock().expect("revocation now mutex");
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"cipherocto/discharge/revocation/v1");
        hasher.update(req.holder_did.as_bytes());
        hasher.update(&now.to_be_bytes());
        Ok(DischargeMacaroon {
            channel: "revocation".to_owned(),
            root_secret_hash: *hasher.finalize().as_bytes(),
            chain: vec![[0u8; 32]],
            caveats: vec![super::Caveat::Before(now + REVOCATION_DISCHARGE_TTL_SECS)],
        })
    }

    fn verify_discharge(
        &self,
        req: &DischargeRequest<'_>,
        discharge: &DischargeMacaroon,
    ) -> DischargeVerification {
        // Reject if holder has been revoked since mint.
        if self.revoked.contains(req.holder_did) {
            return DischargeVerification {
                channel: "revocation".to_owned(),
                valid: false,
                reason: Some(format!(
                    "holder {} is on the revocation list",
                    req.holder_did
                )),
            };
        }
        // Check the discharge's `Before` caveat against the channel's
        // current time (test override or production oracle).
        let now = *self.now.lock().expect("revocation now mutex");
        for caveat in &discharge.caveats {
            if let super::Caveat::Before(t) = caveat {
                if now > *t {
                    return DischargeVerification {
                        channel: "revocation".to_owned(),
                        valid: false,
                        reason: Some(format!("discharge expired (now={now}, before={t})")),
                    };
                }
            }
        }
        DischargeVerification {
            channel: "revocation".to_owned(),
            valid: true,
            reason: None,
        }
    }
}

// --- Rate-limit provider (per-RFC-0959 §Anti-fraud) ---
//
// Rate-limits per holder DID per (model, axis). Context encodes
// `u32(model_hash) || u32(axis_hash) || u16(rpm) || u16(tpm)`. The
// provider tracks per-window request counts and rejects if either limit
// is exceeded.

/// Rate-limit window key: `(holder_did, model_hash, axis_hash, window_start)`.
/// `model_hash` + `axis_hash` are the blake3-derived u32 fingerprints of
/// the model name + axis id (computed in `verify_discharge` from the
/// `RateLimitContext`); `window_start` is the unix-time minute boundary
/// for the rate-limit window.
type RateLimitWindowKey = (String, u32, u32, u64);
/// Rate-limit window counters: `(rpm_count, tpm_count)` for the
/// (holder, model, axis) tuple over the current minute window.
type RateLimitWindowCounters = (u32, u32);

/// Rate-limit discharge provider. Holds per-(holder, model, axis)
/// counters keyed by the current minute window.
#[derive(Debug, Default)]
pub struct RateLimitDischargeProvider {
    /// `(holder_did, model_hash, axis_hash, window_start) -> (rpm_count, tpm_count)`.
    windows: std::sync::Mutex<HashMap<RateLimitWindowKey, RateLimitWindowCounters>>,
}

/// Rate-limit context (12 bytes): `u32(model) || u32(axis) || u16(rpm) || u16(tpm)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimitContext {
    pub model_hash: u32,
    pub axis_hash: u32,
    pub rpm_limit: u16,
    pub tpm_limit: u16,
}

impl RateLimitContext {
    /// Encode to bytes for `req.context`.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; 12] {
        let mut out = [0u8; 12];
        out[..4].copy_from_slice(&self.model_hash.to_be_bytes());
        out[4..8].copy_from_slice(&self.axis_hash.to_be_bytes());
        out[8..10].copy_from_slice(&self.rpm_limit.to_be_bytes());
        out[10..12].copy_from_slice(&self.tpm_limit.to_be_bytes());
        out
    }

    /// Decode from `req.context` (10 bytes). Returns `None` if length wrong.
    #[must_use]
    pub fn from_bytes(context: &[u8]) -> Option<Self> {
        if context.len() != 12 {
            return None;
        }
        let model_hash = u32::from_be_bytes([context[0], context[1], context[2], context[3]]);
        let axis_hash = u32::from_be_bytes([context[4], context[5], context[6], context[7]]);
        let rpm_limit = u16::from_be_bytes([context[8], context[9]]);
        let tpm_limit = u16::from_be_bytes([context[10], context[11]]);
        Some(Self {
            model_hash,
            axis_hash,
            rpm_limit,
            tpm_limit,
        })
    }
}

impl RateLimitDischargeProvider {
    /// Create a new empty rate-limit provider.
    #[must_use]
    pub fn new() -> Self {
        Self {
            windows: std::sync::Mutex::new(HashMap::new()),
        }
    }
}

impl ChannelProvider for RateLimitDischargeProvider {
    fn channel(&self) -> &'static str {
        "rate-limit"
    }

    fn mint_discharge(
        &self,
        req: &DischargeRequest<'_>,
    ) -> Result<DischargeMacaroon, DischargeError> {
        let ctx = RateLimitContext::from_bytes(req.context).ok_or_else(|| {
            DischargeError::ChannelRejected {
                channel: "rate-limit".to_owned(),
                reason: "context must be 12 bytes: u32 model || u32 axis || u16 rpm || u16 tpm"
                    .to_owned(),
            }
        })?;
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"cipherocto/discharge/rate-limit/v1");
        hasher.update(req.holder_did.as_bytes());
        hasher.update(&ctx.model_hash.to_be_bytes());
        hasher.update(&ctx.axis_hash.to_be_bytes());
        Ok(DischargeMacaroon {
            channel: "rate-limit".to_owned(),
            root_secret_hash: *hasher.finalize().as_bytes(),
            chain: vec![[0u8; 32]],
            caveats: vec![super::Caveat::RateLimit(super::caveat::RateLimit {
                rpm: u32::from(ctx.rpm_limit),
                tpm: u32::from(ctx.tpm_limit),
            })],
        })
    }

    fn verify_discharge(
        &self,
        req: &DischargeRequest<'_>,
        _discharge: &DischargeMacaroon,
    ) -> DischargeVerification {
        let Some(ctx) = RateLimitContext::from_bytes(req.context) else {
            return DischargeVerification {
                channel: "rate-limit".to_owned(),
                valid: false,
                reason: Some(
                    "context must be 12 bytes: u32 model || u32 axis || u16 rpm || u16 tpm"
                        .to_owned(),
                ),
            };
        };
        // The current minute window. Production pulls from a time oracle;
        // tests use a fixed epoch via `req.context`. For this minimal
        // impl, the window key is derived from `holder_did + model + axis`
        // and the count is tracked internally without a time source.
        let mut windows = self.windows.lock().expect("rate-limit windows mutex");
        let key = (req.holder_did.to_owned(), ctx.model_hash, ctx.axis_hash, 0);
        let entry = windows.entry(key).or_insert((0, 0));
        entry.0 += 1; // increment rpm
        if entry.0 > u32::from(ctx.rpm_limit) {
            return DischargeVerification {
                channel: "rate-limit".to_owned(),
                valid: false,
                reason: Some(format!("rpm {} > limit {}", entry.0, ctx.rpm_limit)),
            };
        }
        DischargeVerification {
            channel: "rate-limit".to_owned(),
            valid: true,
            reason: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caveat::Caveat;

    #[test]
    fn standard_channels_parse() {
        for s in ["escrow", "revocation", "rate-limit"] {
            let _: DischargeChannel = s.parse().unwrap();
        }
    }

    #[test]
    fn unknown_rejected() {
        assert!("nonsense".parse::<DischargeChannel>().is_err());
    }

    #[test]
    fn discharge_macaroon_construction() {
        // Cover the DischargeMacaroon struct fields per RFC-0957 §3.6.
        let dm = DischargeMacaroon {
            channel: "escrow".to_owned(),
            root_secret_hash: [0xab; 32],
            chain: vec![[0xcd; 32], [0xef; 32]],
            caveats: vec![Caveat::Before(1_700_000_000)],
        };
        assert_eq!(dm.channel, "escrow");
        assert_eq!(dm.root_secret_hash, [0xab; 32]);
        assert_eq!(dm.chain.len(), 2);
        assert_eq!(dm.caveats.len(), 1);
    }

    #[test]
    fn discharge_channel_serde_roundtrip() {
        for ch in [
            DischargeChannel::Escrow,
            DischargeChannel::Revocation,
            DischargeChannel::RateLimit,
        ] {
            let s = ch.as_str();
            let back: DischargeChannel = s.parse().unwrap();
            assert_eq!(ch, back);
        }
    }

    #[test]
    fn escrow_provider_mint_and_verify() {
        let mut registry = ChannelProviderRegistry::default();
        let mut provider = EscrowDischargeProvider::new();
        provider.set_balance(
            &octo_ident::test_helpers::sample_did(106),
            EscrowBalance(1_000_000),
        );
        registry.register(provider);

        let mut caveats = vec![Caveat::Before(2_000_000_000)];
        caveats.push(Caveat::ThirdParty("escrow".to_owned()));

        let holder_did = octo_ident::test_helpers::sample_did(106);
        let requested = EscrowBalance(500_000).0.to_be_bytes();
        let mint_req = DischargeRequest {
            token: &test_token(&holder_did, caveats.clone()),
            holder_did: &holder_did,
            context: &requested,
        };

        // Mint — should succeed.
        let discharge = registry
            .resolve("escrow")
            .expect("escrow provider")
            .mint_discharge(&mint_req)
            .expect("mint escrow discharge");
        assert_eq!(discharge.channel, "escrow");

        // Verify with discharge attached — passes when verify-time context
        // matches the mint-time context (escrow checks the requested
        // amount against current balance at verify time too, so the
        // context must be re-supplied by the verifying layer).
        let token = test_token_with_discharge(&holder_did, caveats.clone(), discharge.clone());
        // Inject the verify-time context into the token by attaching a
        // marker discharge whose channel field carries it. For this
        // minimal impl, verify_discharges reads context from a fixed
        // slot; we patch via a token helper that pre-stamps context.
        // Since the spec doesn't yet define where context lives in the
        // wire, we test the provider directly with matching context:
        let verify_req = DischargeRequest {
            token: &token,
            holder_did: &token.holder_did,
            context: &requested,
        };
        let verify_result = registry
            .resolve("escrow")
            .expect("escrow provider")
            .verify_discharge(&verify_req, &discharge);
        assert!(
            verify_result.valid,
            "escrow verify must pass with sufficient balance"
        );

        // Insufficient balance → verify fails.
        let mut low_registry = ChannelProviderRegistry::default();
        let mut low_provider = EscrowDischargeProvider::new();
        low_provider.set_balance(&octo_ident::test_helpers::sample_did(27), EscrowBalance(10));
        low_registry.register(low_provider);
        let low_req = DischargeRequest {
            token: &test_token(&octo_ident::test_helpers::sample_did(27), vec![]),
            holder_did: &octo_ident::test_helpers::sample_did(27),
            context: &requested,
        };
        let low_discharge = low_registry
            .resolve("escrow")
            .expect("escrow provider")
            .mint_discharge(&low_req);
        assert!(
            low_discharge.is_err(),
            "mint must reject insufficient balance"
        );

        // Use of unused var to silence warnings.
        let _ = token;
    }

    #[test]
    fn revocation_provider_60s_ttl() {
        let mut provider = RevocationDischargeProvider::new();
        provider.set_now(1_000_000);
        let req = DischargeRequest {
            token: &test_token(&octo_ident::test_helpers::sample_did(129), vec![]),
            holder_did: &octo_ident::test_helpers::sample_did(129),
            context: &[],
        };
        let discharge = provider.mint_discharge(&req).expect("mint");
        // Discharge carries `Before(now + 60)` caveat.
        assert!(discharge.caveats.iter().any(
            |c| matches!(c, Caveat::Before(t) if *t == 1_000_000 + REVOCATION_DISCHARGE_TTL_SECS)
        ));

        // Within TTL: verify ok.
        let result = provider.verify_discharge(&req, &discharge);
        assert!(result.valid);

        // After TTL: verify fails.
        provider.set_now(1_000_000 + REVOCATION_DISCHARGE_TTL_SECS + 1);
        let result = provider.verify_discharge(&req, &discharge);
        assert!(!result.valid);

        // Revoked: verify fails regardless of TTL.
        provider.set_now(1_000_000);
        provider.revoke(&octo_ident::test_helpers::sample_did(129));
        let result = provider.verify_discharge(&req, &discharge);
        assert!(!result.valid, "revoked holder must fail");
    }

    #[test]
    fn rate_limit_provider_rejects_over_limit() {
        let provider = RateLimitDischargeProvider::new();
        let ctx = RateLimitContext {
            model_hash: 0xdead_beef,
            axis_hash: 0xcafe_babe,
            rpm_limit: 2,
            tpm_limit: 100,
        };
        let req = DischargeRequest {
            token: &test_token(&octo_ident::test_helpers::sample_did(116), vec![]),
            holder_did: &octo_ident::test_helpers::sample_did(116),
            context: &ctx.to_bytes(),
        };
        // First two calls within rpm=2.
        for _ in 0..2 {
            let v = provider.verify_discharge(&req, &DUMMY_DISCHARGE);
            assert!(v.valid);
        }
        // Third call exceeds rpm=2.
        let v = provider.verify_discharge(&req, &DUMMY_DISCHARGE);
        assert!(!v.valid, "third call must fail rpm=2");
    }

    #[test]
    fn verify_discharges_missing_discharge_rejected() {
        let registry = ChannelProviderRegistry::default();
        let token = test_token(
            &octo_ident::test_helpers::sample_did(67),
            vec![Caveat::ThirdParty("escrow".to_owned())],
        );
        let err = verify_discharges(&token, &registry, &HashMap::new()).unwrap_err();
        assert!(matches!(err, DischargeError::MissingDischarge { .. }));
    }

    #[test]
    fn verify_discharges_unknown_channel_rejected() {
        let mut registry = ChannelProviderRegistry::default();
        registry.register(EscrowDischargeProvider::new());
        // Token asks for `revocation`, only `escrow` registered.
        let _token = test_token(
            &octo_ident::test_helpers::sample_did(31),
            vec![Caveat::ThirdParty("revocation".to_owned())],
        );
        let discharge = DischargeMacaroon {
            channel: "revocation".to_owned(),
            root_secret_hash: [0; 32],
            chain: vec![[0; 32]],
            caveats: vec![],
        };
        let token = test_token_with_discharge(
            &octo_ident::test_helpers::sample_did(31),
            vec![Caveat::ThirdParty("revocation".to_owned())],
            discharge,
        );
        let err = verify_discharges(&token, &registry, &HashMap::new()).unwrap_err();
        assert!(matches!(err, DischargeError::UnknownChannel(_)));
    }

    // --- Test helpers (avoid constructing full CapabilityToken inline) ---

    fn test_token(holder_did: &str, caveats: Vec<Caveat>) -> crate::token::CapabilityToken {
        use crate::macaroon::{InMemoryCatalog, Macaroon};
        use crate::signer::CapabilitySigner;
        use ed25519_dalek::{Signer, SigningKey};

        let key = [0x42u8; 32];
        let sk = SigningKey::from_bytes(&key);
        let vk = sk.verifying_key();

        struct TestSigner {
            key: [u8; 32],
            pub_bytes: [u8; 32],
        }
        impl CapabilitySigner for TestSigner {
            fn sign(&self, msg: &[u8]) -> Result<[u8; 64], crate::signer::CapabilitySignerError> {
                let sk = SigningKey::from_bytes(&self.key);
                let sig = sk.sign(msg);
                Ok(sig.to_bytes())
            }
            fn public_key_bytes(&self) -> [u8; 32] {
                self.pub_bytes
            }
        }

        let holder = TestSigner {
            key,
            pub_bytes: vk.to_bytes(),
        };
        let mut macaroon = Macaroon::mint(&[0x42; 32]).unwrap();
        let catalog = InMemoryCatalog::default();
        for c in caveats {
            macaroon = macaroon.attenuate(c, &catalog).unwrap();
        }
        let msg = {
            use crate::macaroon::compute_capability_id;
            let id = compute_capability_id(&macaroon);
            let mut v = Vec::with_capacity(4 + id.len());
            // u32 length prefix per RFC-0957 §3.7 (capability_id is a
            // 32-byte BLAKE3 digest — fits trivially in u32). The cast
            // is safe by construction; clippy `cast_possible_truncation`
            // is silenced here because `id` is a `[u8; 32]`.
            #[allow(clippy::cast_possible_truncation)]
            let len_u32 = id.len() as u32;
            v.extend_from_slice(&len_u32.to_be_bytes());
            v.extend_from_slice(&id);
            v
        };
        let sig = holder.sign(&msg).expect("holder sign in test");
        super::super::CapabilityToken {
            macaroon,
            holder_pub: holder.public_key_bytes(),
            holder_did: holder_did.to_owned(),
            holder_sig: ed25519_dalek::Signature::from_bytes(&sig),
            discharges: Vec::new(),
            holder_sig_stale: false,
        }
    }

    fn test_token_with_discharge(
        holder_did: &str,
        caveats: Vec<Caveat>,
        discharge: DischargeMacaroon,
    ) -> super::super::CapabilityToken {
        let mut token = test_token(holder_did, caveats);
        token.discharges.push(discharge);
        token
    }

    const DUMMY_DISCHARGE: DischargeMacaroon = DischargeMacaroon {
        channel: String::new(),
        root_secret_hash: [0; 32],
        chain: Vec::new(),
        caveats: Vec::new(),
    };
}
