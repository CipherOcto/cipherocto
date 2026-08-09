# RFC-0009: Identity Management

## Status

Accepted

> **Promotion note (2026-07-19):** Promoted from `planned/process/0009-identity-management.md` → `draft/process/0009-identity-management.md` as part of S01 wallet foundation work. Substrate scope clarified: this RFC owns the **Ed25519 identity substrate** (identity key format, NodeType, vault, capability key derivation). The **Stark Curve transaction substrate** lives in RFC-0102. Capability token format (macaroon v1, HMAC-BLAKE3) lives in RFC-0957 (substrate availability targeted S02; assumption tracked in §Dependency Validation).

## Authors

- Author: @cipherocto (S01 wallet foundation work)
- Contributor: @mmacedoeu (substrate scope clarification 2026-07-19; Ed25519 substrate split from RFC-0102)

## Maintainers

- Maintainer: @mmacedoeu
- Maintainer: @cipherocto

## Summary

Define the core identity model for the CipherOcto network. The `Identity` struct in `octo-core` currently has a `public_key: [u8; 32]` placeholder (zeroed). This RFC specifies the full identity lifecycle: key generation, storage, verification, NodeType taxonomy, provider-key vault separation, and capability key derivation.

## Why Needed

The `octo-core` crate defines `Identity` as a foundational type used across the project:

- `octo-core` — core `Identity` struct with `id` and `public_key`
- `octo-cli` — creates identities, displays them to users
- `octo-registry` — retrieves user identity
- `routing.rs` — re-exports `Identity` for routing decisions

Without a proper identity specification:

1. **Key format is undefined** — what encoding? What curve? What algorithm?
2. **Key generation is unspecified** — how are keypairs created?
3. **Verification is impossible** — can't verify signatures without knowing the key format
4. **Cross-crate contracts are implicit** — no formal interface between octo-core, octo-cli, and octo-registry
5. **NodeType taxonomy missing** — wholesale, self-host, hybrid node roles unspecified
6. **Provider keys leak into identity** — no separation between wallet identity and third-party API keys

## Scope

### In Scope

- Identity key format: Ed25519 (32 bytes raw, multibase(z) encoding, W3C DID-aligned)
- Key generation process (CSPRNG via `OsRng`)
- Public key serialization / deserialization (multibase)
- Identity verification (Ed25519 signature verification using public_key)
- **NodeType taxonomy** — `Wholesale | SelfHost | Hybrid`
- **Provider-key vault** — encrypted storage separate from identity keys (Argon2id + AES-256-GCM)
- **Capability key derivation** — HKDF-BLAKE3 over identity seed per (audience_did, channel_id)
- Integration with RFC-0002 (`agent.identity.public_key`)
- Integration with RFC-0949 (`IdentityProvider`)
- Cross-link with RFC-0102 (Stark Curve substrate) and RFC-0957 (capability token format)

### Out of Scope

- SSO provider implementation details (covered by RFC-0949)
- Agent capability model (covered by RFC-0002)
- **Stark Curve / STARK transaction substrate** (covered by RFC-0102)
- **Capability token wire format, attenuation, discharge** (covered by RFC-0957)
- On-chain identity storage (future protocol phase)

## Dependencies

**Requires:**

- RFC-0102 — defines the parent wallet crate; this RFC specifies identity substrate, RFC-0102 specifies transaction substrate

**Optional:**

- RFC-0002 — agent identity uses same key format
- RFC-0949 — IdentityProvider integration
- RFC-0957 — Capability Token Format; uses this RFC's Ed25519 substrate for holder signatures (substrate availability targeted S02; assumption tracked in §Dependency Validation)

## Proposed Specification

### Identity Struct

```rust
/// Identity is the sovereign cryptographic identity of a CipherOcto node or user.
/// Backed by Ed25519 keypair (RFC-0102 §Stark Curve is for STARK transactions only;
/// this RFC owns the Ed25519 substrate).
pub struct Identity {
    pub id: String,                  // W3C DID: `did:octo:<multibase(z)-encoded-32-bytes>`
    pub public_key: [u8; 32],        // raw Ed25519 public key
}

/// Canonical serialization for Identity (Class A per RFC-0008).
/// Used as input to signature schemes and cross-implementation determinism tests.
/// Format: `b"cipherocto/identity/v1" || DID.as_bytes() || public_key` (length-delimited).
pub fn canonical_ser(identity: &Identity) -> Vec<u8> {
    let mut out = Vec::with_capacity(20 + identity.id.len() + 32);
    out.extend_from_slice(b"cipherocto/identity/v1");
    out.extend_from_slice(&(identity.id.len() as u32).to_be_bytes());
    out.extend_from_slice(identity.id.as_bytes());
    out.extend_from_slice(&identity.public_key);
    out
}
```

> **Note:** The original `octo-core/src/identity.rs` declared `pub public_key: [u8; 32]` but with zero-value placeholder. S01 wallet foundation implementation (mission `0102-a-wallet-foundation.md`) replaces the placeholder with a real Ed25519 keypair via the `IdentityKey` newtype wrapper defined in `crates/octo-wallet/src/identity.rs`.

### Identity Key Format

- **Algorithm:** Ed25519 (RFC 8032)
- **Curve:** Edwards25519
- **Encoding:** multibase(z) of raw 32-byte public key (W3C DID-aligned)
- **DID format:** `did:octo:` + multibase(z) of raw 32 bytes (e.g., `did:octo:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK`)

#### Open question resolutions (amended 2026-07-19):

1. **id format** = `did:octo:` + multibase(z) — W3C DID-compatible
2. **public_key encoding** = multibase(z) of raw 32 bytes (single canonical encoding; no Base64 or hex escape hatches)
3. **Key rotation** = supported via successor linkage per §Lifecycle Requirements below: holder signs `Ed25519(old_seed, "rotate" || new_pubkey_bytes)`, emits `IdentityRotated` event, marks old as `successor: new_did` for a 24-hour grace window. After grace window, old identity is marked `deprecated` and rejected at verification.
4. **Substrate scope** = Ed25519 for identity + capability signatures; Stark Curve for STARK transactions (RFC-0102)

### Key Generation

```
1. Generate Ed25519 keypair via OsRng (CSPRNG)
2. Encode public key as multibase(z) of raw 32 bytes
3. Compute DID: `did:octo:` + multibase(z) encoding
4. Create Identity { id: did, public_key: encoded_bytes }
5. Store private key in wallet vault (NEVER in Identity struct)

Note: dual-substrate (Identity ↔ Stark Curve keypair) link is stored in
wallet metadata at `~/.config/cipherocto/wallet.meta.json`, NOT in Identity
struct. Identity is Ed25519-only per its substrate scope. Wallet metadata
key: `stark_curve_pubkey: <hex>`; cross-validated against RFC-0102 wallet
on capability mint and settlement signing.
```

### Verification

```
1. Verify DID format: prefix == "did:octo:", suffix decodes as multibase(z) → 32 bytes
2. Verify Ed25519 signature against Identity.public_key + canonical_ser(message)
3. Return Result<(), VerificationError>

Note: Step 1 in earlier draft redundantly compared parsed-DID bytes against
Identity.public_key. Since Identity is the source of truth, the comparison is
implicit. Verification checks (a) DID format, (b) signature validity.
```

### Node — NodeType Taxonomy

CipherOcto nodes have distinct operational roles that affect capability mint authority:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeType {
    /// Wholesale: resells quota purchased from a provider plan.
    /// CANNOT mint ZK-class capabilities (loud failure).
    /// Holds provider keys in vault; opaque boundary to provider.
    Wholesale,

    /// SelfHost: owns inference infrastructure.
    /// CAN mint ZK-class capabilities (with PoI proof binding).
    /// Holds its own model weights; no provider keys in vault.
    SelfHost,

    /// Hybrid: combines wholesale + self-host roles.
    /// Optional flag per capability class.
    Hybrid,
}
```

**ZK-class capability gating rule:** Wholesale nodes MUST reject ZK-class capability mint with error `NodeTypeCannotMintZKCap`. Self-host nodes mint ZK-class by default (binds output hash via PoI). Hybrid nodes choose per-capability.

### Vault — Provider-Key Handling

The provider-key vault is **separate** from the identity key. Identity keys are managed by the wallet's primary keystore; provider keys (third-party API keys like OpenAI/Anthropic) live in a dedicated encrypted vault directory at `~/.config/cipherocto/vault/`.

**Vault layout:** file-per-slot on disk; one file per provider key. The in-memory `Vault` struct caches `EncryptedBlob`s on demand and flushes on `put`. Slot files are named `<slot_id>.vault` (hex-encoded slot_id, not arbitrary user input).

```rust
pub struct Vault {
    /// Directory containing `<slot_id>.vault` files; resolved from
    /// `~/.config/cipherocto/vault/` via `directories::ProjectDirs`.
    slots_dir: PathBuf,
    /// In-memory cache: slot_id -> EncryptedBlob. Cache is volatile; reloaded from disk on open.
    cache: HashMap<String, EncryptedBlob>,
}

pub struct EncryptedBlob {
    /// Slot identifier (hex-encoded; sanitized at API boundary).
    slot_id: String,
    /// Argon2id salt (16 bytes).
    salt: [u8; 16],
    /// AES-256-GCM nonce (12 bytes).
    nonce: [u8; 12],
    /// AES-256-GCM ciphertext + 16-byte AEAD tag appended.
    ciphertext: Vec<u8>,
}

pub enum VaultError {
    /// Slot ID not found in vault directory.
    SlotNotFound(String),
    /// Passphrase incorrect OR ciphertext tampered (AEAD tag mismatch).
    DecryptionFailed,
    /// Argon2id KDF exceeded 5-second budget (per §Lifecycle Time Bounds).
    KdfTimeout,
    /// I/O error reading/writing slot file.
    Io(std::io::Error),
    /// Slot ID failed sanitization (path traversal attempt, etc.).
    InvalidSlotId(String),
}

impl Vault {
    /// Encrypt provider key with Argon2id(passphrase) + AES-256-GCM.
    /// Argon2id params: m=64 MiB, t=3, p=4 (OWASP 2024).
    /// Acquires `flock(LOCK_EX)` on slot file during mutation.
    pub fn put(&mut self, slot_id: &str, plaintext: &[u8], passphrase: &str) -> Result<(), VaultError>;

    /// Returns one-shot borrow (DecryptedHandle) — plaintext zeroize-on-drop.
    /// Acquires `flock(LOCK_SH)` on slot file during read.
    /// Vault file contents mapped into process memory use `mlock(2)` on Linux
    /// or `VirtualLock` on Windows immediately after read.
    pub fn get(&self, slot_id: &str, passphrase: &str) -> Result<DecryptedHandle<'_>, VaultError>;

    /// List slot IDs in the vault directory (no plaintext, no passphrase).
    pub fn list(&self) -> Result<Vec<String>, VaultError>;
}

/// One-shot decrypted handle. Dereferences to plaintext bytes.
/// `Drop` impl calls `zeroize::Zeroize::zeroize` on the buffer before deallocation.
pub struct DecryptedHandle<'a> { /* opaque */ }
impl<'a> Deref for DecryptedHandle<'a> { type Target = [u8]; /* ... */ }
```

**Capability token invariant:** Capability tokens (RFC-0957) reference provider keys via `ProviderKeyRef { provider, slot }`; the actual key bytes never appear in the token. Vault borrow happens at egress (S04), one-shot, and never crosses the provider boundary (per S04 ingress/egress transform).

### Capability Keys

Capability keys are **derived** from the identity seed via HKDF-BLAKE3, never independently generated. Each `(audience_did, channel_id)` pair produces an independent capability key (SimpleX-style pairwise unlinkability).

```rust
pub type CapabilityKey = [u8; 32];

/// Derive capability key via HKDF-BLAKE3.
/// - salt   = identity seed (32 bytes from ed25519-dalek SigningKey.to_bytes())
/// - info   = `b"cipherocto/cap/v1/" + channel_id.as_bytes()` (versioned info string)
/// - ikm    = `audience_did.as_bytes()` (input keying material = audience DID)
/// - output = 32-byte CapabilityKey
///
/// Deterministic per (identity, audience, channel).
/// Different audiences OR different channels → different CapabilityKey (unlinkable).
pub fn derive_capability_key(
    identity_key: &IdentityKey,
    audience_did: &DID,
    channel_id: &ChannelId,
) -> CapabilityKey {
    let salt = identity_key.seed_bytes(); // ed25519-dalek SigningKey.to_bytes() — 32 bytes
    let mut info = Vec::with_capacity(20 + channel_id.as_bytes().len());
    info.extend_from_slice(b"cipherocto/cap/v1/");
    info.extend_from_slice(channel_id.as_bytes());
    let mut okm = [0u8; 32];
    hkdf_blake3(&salt, audience_did.as_bytes(), &info, &mut okm);
    okm
}

/// Holder signature for capability tokens (RFC-0957):
/// Ed25519 over BLAKE3(macaroon_root) using identity_key.sign
pub fn holder_sign(identity_key: &IdentityKey, root_hash: BLAKE3) -> Ed25519Signature;
```

**HKDF info string versioning:** constant `cipherocto/cap/v1/` prefix per S01 wallet. Future versions bump to `cipherocto/cap/v2/` etc; old keys remain derivable but new tokens use new info string. The audience DID acts as the IKM (input keying material), distinct from salt — ensures the same identity produces different CapabilityKeys for different audiences even with the same channel_id.

**Sub-capability derivation:** downstream parties (e.g., a downstream node receiving an attenuated capability) derive their own capability key as child of the root per their own `(audience_did, channel_id)` pair. The derivation is per-call; no persistent state.

### HsmAdapter Integration (v1.1 amendment, 2026-08-08)

All signing operations in `octo-wallet` MUST route through the `HsmAdapter` trait defined at `crates/octo-wallet/src/hsm.rs:33` rather than direct `ed25519_dalek::SigningKey` access. The `HsmAdapter` contract is:

```rust
pub trait HsmAdapter: Send + Sync {
    fn get_public_key(&self) -> Result<[u8; 32], HsmError>;
    fn sign(&self, msg: &[u8]) -> Result<[u8; 64], HsmError>;
}
```

Concrete impls:
- **`InMemorySigner`** — wraps identity seed directly; MVP default; signs in-process
- **`LedgerSigner`** — production stub for Ledger device via APDU over USB HID; delegates signing to hardware secure element
- **Future:** `YubiHsmSigner`, `TpmSigner`, `TeeSigner` (per RFC-0853 §F2)

`IdentityKey::sign(msg)` MUST delegate to `self.signer.sign(msg)` where `self.signer: Arc<dyn HsmAdapter>`. Today (pre-v1.1) the wallet bypasses this abstraction and signs directly via `ed25519-dalek::SigningKey::from_bytes(...).sign(msg)` at `crates/octo-wallet/src/identity.rs:71`. This is a known gap (audit 2026-08-08); closure tracked by `missions/open/0009-a-hsm-routing.md`.

**Why:** hardware wallets (Ledger, YubiHSM, TEE) hold the private key in a secure element. Direct `ed25519-dalek` access exposes the seed to host memory and breaks the hardware-bound signing contract. With `HsmAdapter` routing, a wallet user can deploy a Ledger device and the wallet transparently delegates signing to the device — no host-side seed exposure.

**Adapter invariant:** the `HsmAdapter::sign` contract guarantees (a) private key never leaves the device; (b) signing is constant-time at the transport layer; (c) public key discoverable via `get_public_key()`. Production adapters MUST enforce all three. `InMemorySigner` does NOT enforce (a) — it is MVP-only and MUST be replaced before production deployment per `crates/octo-wallet/src/hsm.rs:48-51`.

**Performance overhead:**
| Adapter | Per-sign latency | Notes |
| --- | --- | --- |
| `InMemorySigner` | ~10 µs (matches ed25519-dalek baseline) | MVP; no security boundary |
| `LedgerSigner` | ~50-100 ms (APDU roundtrip + on-device confirmation) | Production; user confirms on-device |
| `YubiHsmSigner` | ~5-15 ms (USB) | Production; no user interaction |
| `TpmSigner` | ~1-5 ms (TEE) | Production; no user interaction |

**Adversary coverage (added v1.1):**
- **A9 — Host-side seed exfiltration.** Today's direct `ed25519_dalek` access leaves the seed in host memory; cold-boot attack or memory dump reveals the private key. **Defense:** `HsmAdapter` routing; seed exists only inside the secure element.
- **A10 — Malicious host signs for the user.** Today, any code with host memory access can call `identity_key.sign(...)` without the user's knowledge. **Defense:** `LedgerSigner` requires explicit on-device confirmation per `HsmError::UserRejected`. Hardware wallets give the user veto power.

### Wallet Audience Validation (v1.1 amendment, 2026-08-08)

`AudienceId::from_str` (and every `AudienceId::new(String)` constructor) MUST call `octo_ident::CanonicalCodec::parse(s, false)` to enforce canonical wire-form parsing at every entry point. `allow_legacy_bare: false` for production paths; `true` only inside `#[cfg(test)]` fixtures where legacy wire form literals exist.

```rust
impl FromStr for AudienceId {
    type Err = DidError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let wire = CanonicalCodec::parse(s, false)?; // RFC-0010 canonical validation
        Ok(AudienceId(wire))
    }
}
```

Today (pre-v1.1) `AudienceId::from_str` accepts any non-empty string (per RFC-0010 §Motivation critique). This is a known gap (audit 2026-08-08); closure tracked by `missions/open/0010-d-wallet-audience-validation.md` (RFC-0010 v1.2 F4).

**Why:** identity verification (RFC-0009 §Verification) checks (a) DID format, (b) signature validity. If the DID format check accepts arbitrary strings, an attacker can substitute `did:octo:attacker-controlled-string` and bypass format verification. Canonical codec validation closes this.

**Cross-reference:** RFC-0010 v1.2 §Future Work F4 mandates this validation at every entry point.

**Reconciliation note (2026-07-19):** Earlier draft of S01 plan §3 Step 5 listed the HKDF pattern as `salt="cipherocto/cap/v1", info=channel_id`. That pattern places the version string in the salt slot, not the info slot, and omits audience DID from IKM. The pattern adopted in this RFC (version in info, audience DID as IKM) is preferred because:
1. Audience unlinkability requires audience DID to participate in derivation — putting it in salt would be semantically wrong (salt should be non-secret random or fixed)
2. HKDF info strings are designed for protocol/version tagging; using info for `cipherocto/cap/v1/` is conventional
3. Salt = identity_seed gives per-identity domain separation; salt = `"cipherocto/cap/v1"` would give global salt with no per-identity uniqueness

S01 plan §3 Step 5 will be updated to match this RFC's pattern during mission claim.

## Open Questions (resolved 2026-07-19)

1. ~~Should `id` be a UUID, DID, or hash of public_key?~~ → **DID**, W3C-aligned (`did:octo:z...`)
2. ~~What encoding for `public_key`?~~ → **multibase(z) of raw 32 bytes**
3. ~~Should Identity support key rotation?~~ → **yes**, via successor linkage (RFC-0853 §12)
4. ~~How does this relate to RFC-0102 wallet keypairs?~~ → **RFC-0102 = Stark Curve transaction substrate; this RFC = Ed25519 identity substrate; both live in wallet crate but own different concerns**

## Economic Analysis

**Not Applicable (N/A) for this RFC.** RFC-0009 is a process RFC defining identity substrate (DID, NodeType, vault, capability key derivation); it does not mint, settle, or transfer tokens. OCTO-W economics are governed by RFC-0959 at the marketplace layer.

**Indirect economics:** identity operations (Ed25519 sign/verify per §Performance Targets; HKDF-BLAKE3 capability key derivation) contribute to node operator's per-invocation compute cost; this is captured at the operator's pricing layer, not at this RFC's layer.

## Performance Targets

> **Reference HW:** modern desktop x86-64 (Intel i7-12700 / AMD Ryzen 7 7700X / Apple M2). Embedded / mobile targets tracked by RFC-0853 §Future Work.

| Operation | Target | Reference HW | Notes |
|-----------|--------|--------------|-------|
| Ed25519 keypair generation | ≤ 1 ms / pair | x86-64 desktop | OsRng-backed; `ed25519-dalek` 2.x baseline |
| Ed25519 sign | ≤ 100 μs / op | x86-64 desktop | Deterministic (RFC 8032); no RNG on hot path |
| Ed25519 verify | ≤ 200 μs / op | x86-64 desktop | Single-verification; batch-verify ≤ 50 μs/op |
| DID encoding (multibase) | ≤ 10 μs / encoding | x86-64 desktop | `z` prefix + base58btc |
| HKDF-BLAKE3 capability key derivation | ≤ 50 μs / derivation | x86-64 desktop | 3 info-block expansion; `BLAKE3` derive-key mode |
| Vault file read (cold) | ≤ 50 ms / file | NVMe SSD | Includes Argon2id KDF (per RFC-0102 §Performance) |
| Vault file read (warm cache) | ≤ 1 ms / file | in-memory `HashMap<String, EncryptedBlob>` | Optional acceleration |
| NodeType lookup (Wholesale / SelfHost / Hybrid) | ≤ 1 μs / lookup | enum match | Const-evaluated at compile time |

## Compatibility

| Surface | Compatibility Target | Notes |
|---------|----------------------|-------|
| W3C DID Core 1.0 | `did:octo:` method (registered with W3C DID Method Registry — pending filing; tracked as IA-4) | `did:octo:z<multibase-b58btc>` |
| Multibase encoding | `z` prefix + base58btc | RFC 0000 multibase table |
| Ed25519 signature | RFC 8032 | 64-byte signature; 32-byte public key |
| HKDF | RFC 5869 + BLAKE3 keyed-hash mode per RFC-0853 | `derive_key(salt, ikm, info, output_len)` |
| BLAKE3 primitive | RFC-0853 §Specification | `blake3::derive_key` for capability keys |
| Cross-RFC canonical_ser | RFC-0126 | `canonical_ser` for Identity struct serialization (added 2026-07-20) |
| Cross-RFC Stark Curve substrate | RFC-0102 | Distinct substrate; same wallet crate hosts both |

## Test Vectors

Per BLUEPRINT.md §Test Vectors: byte-exact fixed inputs/outputs that implementations MUST match.

### TV-1: DID encoding (multibase)

```text
Vector: did-encoding-known-001
Input:
  public_key_raw: 0x<32 bytes; see crates/octo-wallet/tests/fixtures/tv1.json>
Expected:
  did: "did:octo:z<base58btc of 0x<32 bytes>>"
  decoding_round_trip: parse(did) → Some(public_key_raw)
Notes: z-prefix = base58btc encoding per multibase spec.
```

### TV-2: Ed25519 sign + verify (RFC 8032 vector)

```text
Vector: ed25519-sign-known-001
Input:
  private_key: 0x9d61b19deffd5a60ba844af492ec2cc4 4449c5697b326919703bac031cae7f60
  public_key:  0xd75a980182b10ab7d54bfed3c964073a 0ee172f3daa62325af021a68f707511a
  message:     "" (empty)
Expected:
  signature: 0xe5564300c360ac729086e2cc806e828a 84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b
Notes: RFC 8032 §7.1 Test 1 (empty message); verifies RFC 8032 compatibility.
```

### TV-3: HKDF-BLAKE3 capability key derivation

```text
Vector: capability-key-derived-known-001
Input:
  identity_seed: 0x00..01 (32 bytes; salt)
  audience_did:  "did:octo:z<base58btc of pubkey B>" (ikm)
  channel_id:    b"test-channel-001" (info)
Expected:
  capability_root: 0x<32 bytes; see crates/octo-wallet/tests/fixtures/tv3.json>
  pairwise_unlinkable: derive(seed, audience_A) ≠ derive(seed, audience_B) for distinct audiences
Notes: BLAKE3 derive-key mode; HKDF-BLAKE3 expands identity_seed + audience + channel into per-audience capability root.
```

### TV-4: Vault file race condition rejection

```text
Vector: vault-race-reject-known-001
Input:
  process_A: open vault file at ~/.config/cipherocto/vault/test.vault (exclusive flock)
  process_B: same path, same time
Expected:
  process_B: Err(WalletError::VaultLocked { path: "...", holder_pid: A })
Notes: Linux flock(2) atomic exclusive; second opener MUST receive typed error.
```

## Alternatives Considered

| Approach | Pros | Cons | Decision |
|----------|------|------|----------|
| **UUID v4** | Universally unique; no coordination | Not self-describing; no public key binding | Rejected — UUIDs don't carry cryptographic identity |
| **Hash(public_key)** as ID | Deterministic; ID is the public key | No channel separation; loses unlinkability across audiences | Rejected — violates pairwise unlinkability invariant |
| **W3C DID with `did:octo:` method (chosen)** | Self-describing; multibase encoding supports rotation; W3C-aligned | Requires DID method registration (tracked separately per IA-4) | **SELECTED** |
| **PGP-style fingerprint** | Established | Not W3C-aligned; rotation requires re-key | Rejected — incompatible with W3C DID Core 1.0 |
| **Onion address (v3)** | Anonymous | Cryptographic ID bound to onion private key; rotation = new address; no channel binding | Rejected — does not satisfy per-audience unlinkability |

## Rationale

### Why Ed25519 over secp256k1?

- Ed25519 has a clean RFC 8032 specification with deterministic nonce (RFC 8032 §5.1.6); no RNG on hot path → fewer implementation pitfalls.
- Ed25519 signatures are 64 bytes vs secp256k1 65 bytes; verification is 2x faster on x86-64.
- Ed25519 has wide library support (RustCrypto, ring, Go stdlib) and mature audit history.

### Why multibase(z) over base64url?

- Multibase is a W3C-aligned encoding that supports prefix-tagged formats; `z` = base58btc (Bitcoin alphabet) chosen for URL-safety + compactness.
- 32 raw bytes encode to ~44 base58btc chars; vs 44 base64url chars + padding handling.
- Multibase allows future migration to `f` (base16) or other prefixes without breaking compatibility (per §Compatibility §Forward Compatibility).

### Why HKDF-BLAKE3 over HKDF-SHA256?

- BLAKE3 is faster (~5x) than SHA-256 on modern x86-64 with SIMD; matches the CipherOcto crypto substrate (RFC-0853 §1.1).
- BLAKE3 keyed-hash mode (`derive_key`) is structurally equivalent to HKDF-Expand with stronger security properties (per BLAKE3 §5.4).
- Capability key derivation already depends on BLAKE3 elsewhere (RFC-0853 §6 Mission Cryptography); HKDF-BLAKE3 reduces crypto diversity.

### Why is NodeType a separate enum rather than a struct field?

- `NodeType { Wholesale, SelfHost, Hybrid }` is enforced at the type level; downstream code (RFC-0958 §ZK Capability Subclass NodeType gating) can match exhaustively.
- A struct field (`node_type: String`) would allow arbitrary values; per RFC-0959 §Adversary A2, string-typed enums leak attack surface.

## Future Work

- **Post-quantum identity substrate (RFC-0853 §F1):** ML-DSA + SLH-DSA when NIST PQC standards stabilize. Crypto-agility hooks via `Signer` trait + curve identifier enum (already in RFC-0102).
- **HSM routing for all signing paths (v1.1 in-flight, 2026-08-08):** `IdentityKey::sign` + capability mint + Ask signing + capability attenuation MUST route through `Arc<dyn HsmAdapter>` rather than direct `ed25519_dalek::SigningKey` access. Implementation mission: `missions/open/0009-a-hsm-routing.md`. Cross-reference: RFC-0009 §HsmAdapter Integration (v1.1 amendment).
- **Canonical DID validation at every entry point (v1.1 in-flight, 2026-08-08):** `AudienceId::from_str` MUST call `octo_ident::CanonicalCodec::parse(s, false)`. Implementation mission: `missions/open/0010-d-wallet-audience-validation.md` (RFC-0010 v1.2 F4). Cross-reference: RFC-0009 §Wallet Audience Validation (v1.1 amendment).
- **DID method registration (IA-4):** File `did:octo:` method specification with W3C DID Method Registry. Out of MVP scope; tracked separately.
- **Capability attenuation protocols beyond pairwise:** Currently 1:1 audience (per §Capability Keys). Future: hierarchical attenuation chains (parent → child → grandchild) with revocation at any level. Tracked by RFC-0957 §Future Work.
- **Vault offline recovery (Phase H of master plan):** Hardware wallet integration via Ledger/Trezor HID. Out of MVP scope.
- **MPC threshold identity (Phase I of master plan):** 2-of-3 + 3-of-5 threshold identity key. Out of MVP scope; tracked by RFC-0853 §F3.

## Related RFCs

- RFC-0002 — Agent Manifest: defines `agent.identity.public_key`
- RFC-0102 — Wallet Cryptography: Stark Curve transaction substrate (sibling RFC)
- RFC-0949 — Enterprise SSO: IdentityProvider integration
- RFC-0932 — Gateway Auth API Key Management
- RFC-0957 — Capability Token Format: uses this RFC's Ed25519 substrate for holder signatures (substrate availability targeted S02; assumption tracked in §Dependency Validation)

## Key Files to Modify

| File | Current State | Action Needed |
|------|--------------|---------------|
| `crates/octo-core/src/identity.rs` | `public_key: [0u8; 32]` placeholder | Replace with real Ed25519 via `IdentityKey` newtype from `octo-wallet` |
| `crates/octo-wallet/src/identity.rs` | (planned, S01) | Define `IdentityKey(Ed25519Keypair)`, `CapabilityKey([u8; 32])`, `derive_capability_key()` |
| `crates/octo-wallet/src/vault.rs` | (planned, S01) | Define `Vault`, `EncryptedBlob`, `DecryptedHandle`, Argon2id + AES-256-GCM |
| `crates/octo-wallet/src/node.rs` | (planned, S01) | Define `NodeType { Wholesale, SelfHost, Hybrid }` enum |
| `crates/octo-cli/src/main.rs` | Creates identity, prints it | Wire to real `octo-wallet init` command |
| `crates/octo-registry/src/lib.rs` | Gets user identity | Wire to real identity storage |

## Implementation Phases

### Phase 1: Core

- [ ] Define `IdentityKey(Ed25519Keypair)` newtype with `Zeroize`-on-`Drop` + REDACTED `Debug` in `crates/octo-wallet/src/identity.rs`
- [ ] Define `CapabilityKey([u8; 32])` + `derive_capability_key(audience, channel) -> CapabilityKey` via HKDF-BLAKE3 (3-part info-block)
- [ ] Define `DID` canonical wire form `did:octo:z<base58btc>` with `to_wire()` + `from_wire()` round-trip
- [ ] Implement `octo-wallet init` CLI command: keypair generation, mnemonic seed backup, DID print
- [ ] Wire `Identity` struct in `crates/octo-core/src/identity.rs` to real Ed25519 (replace `[u8; 32]` placeholder)
- [ ] 4 test vectors in `crates/octo-wallet/tests/fixtures/identity/`: TV-1 DID multibase, TV-2 RFC 8032 Ed25519 #1, TV-3 HKDF-BLAKE3 capability key, TV-4 vault race rejection

### Phase 2: Vault + NodeType

- [ ] Define `Vault`, `EncryptedBlob`, `DecryptedHandle` in `crates/octo-wallet/src/vault.rs` (Argon2id + AES-256-GCM)
- [ ] Define `NodeType { Wholesale, SelfHost, Hybrid }` enum in `crates/octo-wallet/src/node.rs`
- [ ] Capability mint authority gating per NodeType (Wholesale: no ZK; SelfHost: ZK ok; Hybrid: per-cap)
- [ ] `flock(LOCK_EX)`/`flock(LOCK_SH)` semantics on vault file access
- [ ] `rotate()` API for identity successor linkage (monotonic successor counter)
- [ ] Mission: `missions/claimed/0102-a-wallet-foundation.md` (S01 — wallet foundation)
- [ ] Plan: `docs/plans/2026-07-19-session-01-wallet-foundation.md`

## Roles and Authorities

> **The "Nothing should be implied" rule (specification layer):** Every actor that affects correctness, security, accountability, or consensus MUST be named with a stable identifier, a defined authority scope, and a typed lifecycle. Inference is a defect.

### Role/Authority Coverage Table

| Role | Identifier | Authority Scope | Lifecycle | Source/Ref |
|------|------------|-----------------|-----------|------------|
| Node Identity | `DID = did:octo:<multibase(z)-32-bytes>` | Identity attestation; holder of private Ed25519 key | `Designated` → `Active` → `Rotating` → `Revoked` (per §Lifecycle) | This RFC §Identity Struct |
| NodeType | `enum { Wholesale, SelfHost, Hybrid }` | (a) Capability mint authority (Wholesale: no ZK; SelfHost: ZK ok; Hybrid: per-cap); (b) Provider-key vault contents; (c) Settlement accounting | `Designated` at init; transitions via admin op only | This RFC §Node |
| Identity Holder | Entity controlling private Ed25519 seed (human or service) | Issue capabilities, rotate identity, decrypt provider vault with passphrase | Stateless w.r.t. protocol; lifecycle owned by holder's OPSEC | This RFC §Vault |
| Vault | `Vault` instance at `~/.config/cipherocto/vault/<slot>.vault` | Decrypt-and-borrow provider key bytes for one-shot use at egress transform | `Created` at first `put`; `Rotated` on passphrase change | This RFC §Vault |

### Out-of-Scope Roles

- **SSO Provider** — covered by RFC-0949 (Enterprise SSO); this RFC consumes `IdentityProvider` interface but does not implement IdP.
- **Stark Curve Key Holder** — covered by RFC-0102; this RFC does not manage Stark Curve keys.

## Adversary Analysis (5-Question Test)

This RFC is security-sensitive (key management, encrypted storage, capability token foundation). All CRITICAL findings MUST be mitigated before RFC acceptance.

### Finding A1: Vault passphrase brute force

1. **Who benefits?** — Attacker who steals the vault file (`~/.config/cipherocto/vault/<slot>.vault`) and possesses offline compute (GPU cluster, ASIC farm).
2. **What does it cost them?** — Argon2id at m=64MiB, t=3, p=4 ≈ 250ms per guess on modern CPU; GPU parallelization ~100x speedup. Effective cost: ~$5 per billion guesses on commodity GPU rental.
3. **What do they gain if successful?** — Plaintext provider key (OpenAI/Anthropic API key); can make API calls billed to victim; can read cached responses if vault also stores cached responses (out of scope for this RFC).
4. **What's our defense?** — Argon2id memory-hard KDF (resists GPU/ASIC); provider key stored ONLY when needed at egress (one-shot borrow); vault file at-rest on disk + zeroize on drop. Audit logging on every `get` call.
5. **What's the residual risk?** — Weak passphrase (user picks "password123"); GPU rental cost decreases over time. **Mitigation:** minimum passphrase length requirement enforced by `octo-wallet init` (12+ chars, dictionary rejection); recommend hardware key derivation or passkey factor (Phase H).

**Verdict:** ACCEPTED with mitigation. Phrase length check is must-have at MVP; hardware factors deferred to Phase H.

### Finding A2: Capability key derivation collision (unlinkability break)

1. **Who benefits?** — Adversary with access to two derived capability keys for the same identity with different audiences; wants to confirm they belong to the same identity.
2. **What does it cost them?** — Zero; observation only.
3. **What do they gain if successful?** — Social graph link: confirm two audience DIDs are served by the same identity (privacy break).
4. **What's our defense?** — HKDF-BLAKE3 with audience DID as IKM produces domain-separated output; per-audience and per-channel keys are statistically independent (256-bit output, BLAKE3 is PRF). Information-theoretic argument: an attacker who observes K1=HKDF(salt, audience1, info) and K2=HKDF(salt, audience2, info') gains zero bits about whether `salt` is the same value (BLAKE3 PRF property).
5. **What's the residual risk?** — Implementation bug in HKDF (e.g., reusing output buffer, not zeroing IKM). **Mitigation:** property test (HKDF roundtrip across 10K random (audience, channel) pairs); zeroize IKM and output buffer on drop.

**Verdict:** MITIGATED via property tests + zeroize. No residual risk if implementation matches spec.

### Finding A3: Ed25519 seed exfiltration via logs/traces

1. **Who benefits?** — Attacker who reads process logs, crash dumps, or memory snapshots.
2. **What does it cost them?** — Local access or log aggregation access.
3. **What do they gain if successful?** — Identity key = full impersonation.
4. **What's our defense?** — `zeroize` on all secret-bearing types; `tracing::Instrument` with `skip` on fields carrying secret material; custom `Debug` impls that redact seed bytes; never serialize private keys to disk in plaintext.
5. **What's the residual risk?** — Compiler optimization removing zeroize calls; debugger attaching to live process. **Mitigation:** verify zeroize in release builds (volatile writes); document threat model excludes live debugger adversary.

**Verdict:** MITIGATED for static log/dump threats. Live debugger out of scope.

### Finding A4: DID method spoofing (`did:octo` not registered)

1. **Who benefits?** — Attacker who controls a competing DID method or who can squat on `octo` method specifier.
2. **What does it cost them?** — Low; method squatting has been done before.
3. **What do they gain if successful?** — Confuse DID resolution; trick consumers into accepting non-CipherOcto DIDs.
4. **What's our defense?** — `did:octo:` method specifier is internal-only at MVP; resolution path is defined by CipherOcto routing layer, not public W3C DID resolver. Document the un-registered status; register method with W3C DID Working Group before public-facing deployments.
5. **What's the residual risk?** — Internal use is safe; external-facing use (third-party apps) needs W3C registration. **Mitigation:** mark `did:octo` as CipherOcto-private at MVP; W3C registration tracked separately.

**Verdict:** ACCEPTED with MVP-internal-only scope. W3C registration is post-MVP.

### Finding A5: Vault file race condition (concurrent put/get)

1. **Who benefits?** — Local attacker with code execution during vault file mutation.
2. **What does it cost them?** — Local access during the race window.
3. **What do they gain if successful?** — Replace ciphertext with attacker-chosen ciphertext; AEAD tag mismatch on next `get` triggers decrypt failure, but attacker may have already bypassed auth via timing.
4. **What's our defense?** — Vault uses file locking (`flock(2)`) on `~/.config/cipherocto/vault/<slot>.vault` during `put` and `get`; AEAD tag verification rejects any tamper before plaintext release; mlock at-rest prevents swap-to-disk mid-operation.
5. **What's the residual risk?** — TOCTOU between lock check and file read on non-POSIX-strict filesystems. **Mitigation:** open with `O_EXCL` semantics during mutation; `O_RDONLY` with `flock(LOCK_SH)` during read.

**Verdict:** MITIGATED via flock + AEAD tag. Accepted.

## Lifecycle Requirements

This RFC defines one stateful actor: `Identity` (DID-bearing keypair). NodeType is also stateful but transitions are admin-driven only.

### Identity Lifecycle State Machine

```mermaid
stateDiagram-v2
    [*] --> Designated: IdentityKey::generate()
    Designated --> Active: First capability minted OR first vault put/get
    Active --> Rotating: holder initiates rotation
    Rotating --> Active: successor linked (RFC-0853 §12 amendment)
    Active --> Revoked: holder burns (private key zeroized)
    Revoked --> [*]
```

| From | To | Trigger | Deterministic? | Side Effects | Signing Requirement |
|------|----|---------|----------------|--------------|---------------------|
| Designated | Active | First `mint_capability` or first `vault_put` | Yes | Record `activated_at_unix` in wallet metadata | n/a |
| Active | Rotating | Holder calls `rotate_identity(successor_key)` | Yes | Emit `IdentityRotated { old_did, new_did }` event; mark old as `successor: new_did` | Holder signs `Ed25519(old_seed, "rotate" \|\| new_pubkey_bytes)` |
| Rotating | Active | Successor linked (signature valid, timestamp within grace 24h) | Yes | Old identity marked `deprecated`; new identity marked `Active` | Holder signs (same as above) |
| Active | Revoked | Holder calls `revoke_identity()` (destructive) | Yes | Zeroize private key bytes; emit `IdentityRevoked { did, reason }` event | Holder signs `Ed25519(seed, "revoke")` |
| Revoked | (terminal) | n/a | n/a | Identity is permanently unusable | n/a |

### Liveness Check

No external liveness check for Identity (off-chain, holder-managed). Vault may emit `VaultAccessEvent` audit log on every `get` call for forensics.

### Recovery Semantics

- **Lost passphrase:** vault unrecoverable; provider keys must be re-issued by provider. Mitigation: mnemonic seed backup for identity keys (separate from vault) per RFC-0102 §MnemonicRecovery.
- **Lost identity seed:** identity unrecoverable; new identity must be generated. DIDs previously associated with old identity become invalid.

### Time Bounds

- Rotation grace period: 24 hours (configurable; per RFC-0853 §12 when finalized)
- Vault decryption timeout: 5 seconds (Argon2id is slow; budget 5s for KDF + AEAD)
- Capability key derivation: 10ms budget

## Determinism Requirements (RFC-0008 Execution Class Mapping)

Per BLUEPRINT.md consistency checklist, every RFC MUST include an RFC-0008 execution class mapping.

| RFC-0009 Component | Execution Class | Justification |
|--------------------|-----------------|---------------|
| Ed25519 key generation | **A** (Protocol Deterministic) | OS RNG is the only non-determinism; keypair itself is deterministic given seed. Output is canonical (32 bytes). |
| Ed25519 signature | **A** | RFC 8032 deterministic signature scheme (no RNG in signing). |
| Identity canonical_ser | **A** | Must be deterministic for cross-implementation verification. |
| Argon2id KDF | **B** (Deterministic Off-Chain) | Deterministic given (passphrase, salt, params); cross-implementation verification requires test vectors. |
| AES-256-GCM encrypt/decrypt | **A** | NIST-standardized, deterministic. |
| HKDF-BLAKE3 | **A** | RFC 5869 + BLAKE3 RFC 7693; deterministic. |
| Vault file I/O | **C** (Probabilistic) | Filesystem timing varies; not consensus-relevant. |
| Capability key derivation | **A** | HKDF is deterministic; same inputs → same output. |

**Determinism contract:** Two implementations of §Identity Key Format + §Capability Keys + §Verification MUST produce identical results for the same inputs. Cross-implementation test vectors included in `crates/octo-wallet/tests/fixtures/determin/`.

## Adversarial Review

| Threat | Impact | Mitigation |
|--------|--------|------------|
| Vault passphrase brute force | High (private key exfiltration → identity theft) | Argon2id KDF (m=64MiB, t=3, p=4); 12+ char minimum + dictionary rejection (per §Performance Targets); recommended hardware factor (Phase H) |
| Capability key derivation collision (unlinkability break) | High (cross-channel correlation of holder activity) | HKDF-BLAKE3 with 3-part info-block; property test across 10K random (audience, channel) pairs; zeroize IKM and output on drop |
| Ed25519 seed exfiltration via logs/traces | Critical (identity theft) | `SecretKey` zeroize-on-drop; `Debug` impls REDACTED (per project rule: "Debug should not leak in full security related data"); compile-time `#[derive(Zeroize)]`; no `Display` impl |
| DID method spoofing (`did:octo` not W3C-registered) | Medium (external-facing identity ambiguity) | Mark `did:octo` as CipherOcto-private at MVP; W3C registration tracked separately (IA-4); inbound `did:` parsers validate method = `octo` |
| Vault file race condition (concurrent put/get) | Medium (data loss or stale read) | `flock(LOCK_EX)` on mutation + `flock(LOCK_SH)` on read; atomic rename (`rename(2)`) on swap; `O_EXCL` semantics during initial write |
| Identity key rotation race (successor overwrite) | Medium (lost successor linkage; recovery impossible) | Monotonic successor counter; rotation only via dedicated `rotate()` API; refuse rotation if successor counter != expected |
| HKDF info-block confusion (cross-tenant capability replay) | High (capability theft via re-derivation) | Canonical info-block: `("octo/capability-key/v1", audience, channel)`; reject unversioned info-blocks at runtime |

## Security Considerations

### Threat Model

- **In scope:** local attacker with file access; network attacker observing capability tokens; offline brute-force of vault passphrase.
- **Out of scope:** live debugger attaching to running process; side-channel attacks on CPU cache timing; supply chain attacks on dependencies.

### Key Handling Rules

1. **Never log private keys.** `tracing::Instrument` fields skip secret material.
2. **Never serialize private keys.** Keystore format is the only persistence path; uses Argon2id encryption at rest.
3. **Zeroize on drop.** All types wrapping secret material implement `Drop` with `zeroize::Zeroize`.
4. **mlock at rest.** Vault file contents mapped into process memory use `mlock(2)` (Linux) or `VirtualLock` (Windows) to prevent swap-to-disk.
5. **No `unsafe`** in wallet crate. `#![forbid(unsafe_code)]` at crate root.
6. **Constant-time comparisons.** All HMAC/signature/tag comparisons use `subtle::ConstantTimeEq`.

### Cryptographic Agility

- Algorithm: Ed25519 (RFC 8032). Future migration to post-quantum (e.g., ML-DSA, SLH-DSA) tracked by RFC-0853 §F1 (post-quantum migration).
- KDF: Argon2id. Future migration to Argon2id-tuned-2027 or successor tracked separately.
- Hash: BLAKE3. SHA-256 / SHA-3 fallback not implemented (BLAKE3 is the only approved hash in CipherOcto).

## Implicit Assumptions Audit

Per BLUEPRINT.md, every RFC MUST include an Implicit Assumptions Audit. Entries with non-trivial blast radius MUST be tracked to closure.

| # | Assumption | Blast Radius | Tracking |
|---|-----------|--------------|----------|
| IA-1 | `did:octo:` is a CipherOcto-private DID method; not registered with W3C | External-facing identity resolution may be ambiguous | W3C registration tracked post-MVP |
| IA-2 | Identity ↔ Stark Curve keypair link is stored in wallet metadata, not in Identity struct | Loss of wallet metadata breaks dual-substrate link | Document backup procedure in wallet user guide |
| IA-3 | Argon2id parameters (m=64MiB, t=3, p=4) are sufficient against 2026 GPU attacks | If params become insufficient, all vault files at risk | Annual review of OWASP recommendations |
| IA-4 | Ed25519 is not yet broken by quantum computers | Quantum adversary can derive private key from public key | RFC-0853 §F1 post-quantum migration |
| IA-5 | `OsRng` on supported platforms is CSPRNG-quality | Compromised RNG = predictable keys | Test OS RNG quality at startup; warn on dev/null entropy |
| IA-6 | Capability key derivation per (audience, channel) provides sufficient unlinkability | If BLAKE3 PRF assumption breaks, linkability broken | Track BLAKE3 cryptanalysis status |

## Dependency Validation

Per BLUEPRINT.md:

| Dependency | Status | Assumption |
|------------|--------|-----------|
| RFC-0102 (Wallet Cryptography) | Draft (amended 2026-07-19; promoted to v0.3 2026-07-20 with §Authors, §Maintainers, §Performance Targets, §Compatibility, §Test Vectors) | Must reach Accepted before this RFC Accepted |
| RFC-0002 (Agent Manifest) | Accepted (assumed; verify at promotion) | None — additive integration |
| RFC-0126 (Deterministic Serialization) | Accepted (canonical_ser substrate for Identity struct serialization; added 2026-07-20) | None — additive integration |
| RFC-0949 (Enterprise SSO) | Accepted | None — additive integration |
| RFC-0957 (Capability Token Format) | **Draft** (S02 work; authored 2026-07-19) | **Assumption: RFC-0957 will reach Accepted before this RFC's full implementation lands.** RFC-0957 must define `Ed25519Signature` holder sig type compatible with this RFC's `holder_sign`. |

## Version History

| Version | Date | Status | Author | Notes |
|---------|------|--------|--------|-------|
| 0.1 | 2026-03-03 | Planned | @cipherocto | Initial placeholder; identified 4 open questions |
| 0.2 | 2026-07-19 | Draft (promoted) | @cipherocto (session-01 wallet foundation work) | Added §Identity Key Format, §Node, §Vault, §Capability Keys; resolved open questions (DID, multibase(z), rotation via successor, substrate split); added §Adversary Analysis (5-Question Test, 5 findings), §Lifecycle Requirements, §Determinism Requirements (RFC-0008 mapping), §Security Considerations, §Implicit Assumptions Audit, §Dependency Validation |
| 0.3 | 2026-07-20 | Draft (acceptance-prep) | @mmacedoeu | Pre-acceptance fixes (BLUEPRINT v1.3 template completeness): added §Authors, §Maintainers; stripped `§` prefix from all mandatory section H2/H3 (was non-standard); added §Performance Targets (8-row latency table), §Compatibility (8-row surface table incl. RFC-0126 canonical_ser + RFC-0853 BLAKE3), §Test Vectors (TV-1 DID multibase, TV-2 RFC 8032 Ed25519 #1, TV-3 HKDF-BLAKE3 capability key, TV-4 vault race rejection); added Cross-RFC dependency to RFC-0126 (Deterministic Serialization) |
| 0.4 | 2026-07-20 | Draft (review-fix) | @mmacedoeu | Review R1 fixes: added §Alternatives Considered (5-row table: UUID v4 / hash(pubkey) / DID chosen / PGP fingerprint / Onion address); added §Rationale (4 sub-sections: Ed25519 over secp256k1, multibase(z) over base64url, HKDF-BLAKE3 over HKDF-SHA256, NodeType as enum); added §Future Work (5 items: PQC identity substrate per RFC-0853 §F1, DID method registration IA-4, hierarchical attenuation, hardware wallet Phase H, MPC Phase I); renamed §Key Files → §Key Files to Modify + §Implementation Reference → §Implementation Phases per BLUEPRT template conventions. |
| 0.5 | 2026-07-20 | Draft (review-fix) | @mmacedoeu | Review R2 fix: added §Economic Analysis (N/A — process RFC defining identity substrate; no direct token mint/settlement; OCTO-W economics governed by RFC-0959 at marketplace layer). |
| 2026-07-20 | **Promoted to Accepted.** 7-day review (initiated 2026-07-19 alongside session-01/02/03/04/05 work) + 2 maintainer approvals (@mmacedoeu + @cipherocto) completed; no blocking objections. Status header updated; file moved via `git mv` from `rfcs/draft/{category}/` to `rfcs/accepted/{category}/`. Pre-acceptance completeness fixes applied (see prior version rows 0.2-0.5/1.1/1.2.0/1.2.1). |
| 1.0 | 2026-08-03 | Accepted (audit) | @mmacedoeu | Audit pass: stripped `(Process)`/`(Numeric)`/`(Economics)` category parens from RFC references + H1 title per CLAUDE.md referencing rule; added §Adversarial Review threat table (template §651 requirement); restructured §Implementation Phases to Phase 1 / Phase 2 with `- [ ]` checkboxes (template §693 requirement); added §Related Use Cases + §Appendices (template §731, §735). |
| 1.1 | 2026-08-08 | Accepted (amendment) | @cipherocto + @mmacedoeu | **HSM routing + canonical DID validation requirements.** Surfaced by 2026-08-08 specialized node protocol research (`docs/research/2026-08-08-specialized-node-protocol-research.md`) + RFC-0871 (Planned). Two amendments: (1) **HSM routing**: all `IdentityKey::sign` paths in `octo-wallet` MUST route through `Arc<dyn HsmAdapter>` (defined at `crates/octo-wallet/src/hsm.rs:33`) rather than direct `ed25519-dalek::SigningKey` access. This enables hardware wallet support (Ledger, YubiHSM, TEE) for capability mint + Ask signing + capability attenuation. Foundation mission: `missions/open/0009-a-hsm-routing.md`. (2) **Canonical DID validation**: `AudienceId::from_str` (and every other `AudienceId::new(String)` constructor) MUST call `octo_ident::CanonicalCodec::parse(s, false)` to enforce canonical wire-form parsing at every entry point. Defense against DID spoofing via arbitrary string substitution. Companion mission: `missions/open/0010-d-wallet-audience-validation.md` (RFC-0010 v1.2 F4). Both amendments are additive (no wire-format change); production parity preserved via `InMemorySigner` default impl. Per RFC-0871 §Implementation Phase 2. |

## Related Use Cases

- [Canonical OctoID Identifier](../../docs/use-cases/canonical-octoid-identifier.md) — sibling use case documenting DID wire format; consumed by RFC-0010 codec crate.

## Appendices

### A. Identity Lifecycle State Machine (detailed)

The state machine defined in §Lifecycle Requirements is reproduced here for cross-RFC copy-paste clarity:

```rust
#[repr(u8)]
enum IdentityLifecycle {
    Designated = 0x00,  // named at init, not yet active
    Active = 0x01,     // identity in use; signing operations live
    Rotating = 0x02,   // successor link established; old key still valid during grace
    Revoked = 0x03,    // identity retired; signature verification rejected
}
```

| From | To | Trigger | Deterministic? | Side Effects | Signing |
|------|----|---------|----------------|--------------|---------|
| Designated | Active | First successful sign | Yes | Emit identity activation event | Self-signature |
| Active | Rotating | `rotate()` API call | Yes | Generate successor keypair; emit rotation event | Old key + new key co-sign |
| Rotating | Active | Grace period elapsed (default 7 days) | Yes | Old key marked legacy; new key takes over | Old key co-sign on handover |
| Active | Revoked | User-initiated revoke OR governance vote | Yes | Vault sealed; capability tokens invalidated | Self-signature + governance envelope |
| Rotating | Revoked | Abort rotation (user choice) | Yes | New key destroyed; old key remains Active | Old key self-signature |

### B. Test Vector Sources

External test vector sources for cross-implementation verification:

- RFC 8032 §7.1 Test 1 (Ed25519 sign/verify known answer)
- BLAKE3 reference vectors (https://github.com/BLAKE3-team/BLAKE3/blob/master/test_vectors/test_vectors.json)
- HKDF RFC 5869 Test Cases 1-3
- Argon2id RFC 9106 reference implementation vectors
- base58btc Bitcoin Alphabet reference table

### C. Mnemonic Seed Backup Format

Identity key mnemonic backup uses BIP-39 English wordlist (2048 words) for human-readable recovery. 12-word default; 24-word option for paranoid mode. Mnemonic maps to 64-byte seed via PBKDF2-HMAC-SHA512 (per BIP-39 spec); seed is the Ed25519 keypair seed input. The mnemonic is NOT the vault passphrase — separate secret.

---

**Submission Date:** 2026-03-03
**Last Updated:** 2026-07-19 (promoted Planned → Draft; added §Node, §Vault, §Capability Keys, §Identity Key Format; substrate scope clarified cross-linking RFC-0102 and RFC-0957)
