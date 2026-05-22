# RFC-0009 (Process): Identity Management

## Status

Planned

## Summary

Define the core identity model for the CipherOcto network. The `Identity` struct in `octo-core` currently has a `public_key: String::new()` placeholder. This RFC specifies the full identity lifecycle: key generation, storage, verification, and integration with existing auth systems.

## Why Needed

The `octo-core` crate defines `Identity` as a foundational type used across the project:

- `octo-core` — core `Identity` struct with `id` and `public_key`
- `octo-cli` — creates identities, displays them to users
- `octo-registry` — retrieves user identity
- `routing.rs` — re-exports `Identity` for routing decisions

Currently, `public_key` is initialized as `String::new()` (MVP placeholder). Without a proper identity specification:

1. **Key format is undefined** — what encoding? What curve? What algorithm?
2. **Key generation is unspecified** — how are keypairs created?
3. **Verification is impossible** — can't verify signatures without knowing the key format
4. **Cross-crate contracts are implicit** — no formal interface between octo-core, octo-cli, and octo-registry

## Scope

### In Scope

- Identity key format (algorithm, encoding, size)
- Key generation process
- Public key serialization/deserialization
- Identity verification (signature verification using public_key)
- Integration with RFC-0002 Agent Manifest (`agent.identity.public_key`)
- Integration with RFC-0949 Enterprise SSO (`IdentityProvider`)

### Out of Scope

- SSO provider implementation details (covered by RFC-0949)
- Agent capability model (covered by RFC-0002)
- Wallet cryptography (covered by RFC-0102)
- On-chain identity storage (future protocol phase)

## Dependencies

**Requires:**

- RFC-0102 (Numeric): Wallet Cryptography — defines key pair format, signature schemes

**Optional:**

- RFC-0002 (Process): Agent Manifest — agent identity uses same key format
- RFC-0949 (Economics): Enterprise SSO — IdentityProvider integration

## Proposed Specification

### Identity Struct

```rust
pub struct Identity {
    pub id: String,           // Unique identifier (UUID or DID)
    pub public_key: String,   // Encoded public key (format TBD by this RFC)
}
```

### Key Format (Draft)

- Algorithm: Ed25519 (or RFC-0102's chosen scheme)
- Encoding: Base64 or hex (TBD)
- Size: 32 bytes raw public key

### Key Generation

```
1. Generate Ed25519 keypair
2. Encode public key per format spec
3. Create Identity { id: uuid(), public_key: encoded }
4. Store private key securely (NOT in Identity struct)
```

### Verification

```
1. Parse public_key from Identity
2. Verify signature against public_key
3. Return bool (valid/invalid)
```

## Open Questions

1. Should `id` be a UUID, DID, or hash of public_key?
2. What encoding for `public_key`? (Base64, hex, PEM)
3. Should Identity support key rotation?
4. How does this relate to RFC-0102 wallet keypairs?

## Related RFCs

- RFC-0002 (Process): Agent Manifest — defines `agent.identity.public_key`
- RFC-0102 (Numeric): Wallet Cryptography — key pair format
- RFC-0949 (Economics): Enterprise SSO — IdentityProvider model
- RFC-0932 (Economics): Gateway Auth API Key Management

## Key Files

| File | Current State | Action Needed |
|------|--------------|---------------|
| `crates/octo-core/src/identity.rs` | `public_key: String::new()` placeholder | Implement per spec |
| `crates/octo-cli/src/main.rs` | Creates identity, prints it | Wire to real key generation |
| `crates/octo-registry/src/lib.rs` | Gets user identity | Wire to real identity storage |
