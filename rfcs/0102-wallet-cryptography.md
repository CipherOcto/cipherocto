# RFC-0102: Wallet Cryptography Specification

## Status
Draft

## Summary

Defines cryptographic primitives for the CipherOcto wallet system, including key management, signature schemes, account models, and secure storage - all compatible with the Starknet/Cairo ecosystem used by Stoolap for ZK proofs.

## Motivation

### Problem Statement

The Quota Router MVE requires a secure wallet implementation to handle OCTO-W, OCTO-D, and OCTO token transactions. Current research establishes the need for cryptographic foundations, but we need a concrete specification before implementation.

### Current State

- Research complete: `docs/research/wallet-technology-research.md`
- Starknet compatibility identified as primary requirement
- Key components: starknet-rs for signing, custom key storage

### Desired State

A complete specification defining:
- Key types and derivation paths
- Transaction signing workflow
- Local key storage format
- Account abstraction interface
- Error handling patterns

### Use Case Link
- [AI Quota Marketplace](../docs/use-cases/ai-quota-marketplace.md)

## Specification

### Data Structures

```rust
/// Starknet FieldElement (32 bytes)
type FieldElement = [u8; 32];

/// Starknet signature component
type Signature = (FieldElement, FieldElement);

/// Wallet key pair
struct KeyPair {
    private_key: FieldElement,
    public_key: FieldElement,
}

/// Account address (contract orEOA)
type Address = FieldElement;

/// Token type in the OCTO ecosystem
#[derive(Clone, Copy, Debug)]
enum Token {
    OCTO,   // Governance token
    OCTO_W, // Wrapped quota (1 token = 1 prompt)
    OCTO_D, // Contributor reward token
}

/// Transaction nonce for replay protection
struct Nonce(u64);

/// Chain identifier
struct ChainId(FieldElement);
```

### Transaction Types

```rust
/// Token transfer transaction
struct Transfer {
    sender: Address,
    receiver: Address,
    token: Token,
    amount: u64,
    nonce: Nonce,
    chain_id: ChainId,
    fee: u64,
}

/// Execute multiple calls atomically
struct Execute {
    calls: Vec<Call>,
    nonce: Nonce,
    chain_id: ChainId,
}

/// Single contract call
struct Call {
    to: Address,
    selector: FieldElement,
    calldata: Vec<FieldElement>,
}
```

### Key Derivation

```rust
/// BIP-32 style derivation trait
trait KeyDerivation {
    /// Derive a child key from parent key and index
    fn derive(&self, seed: &FieldElement, index: u32) -> FieldElement;
}

/// Starknet-specific derivation (non-BIP-44)
struct StarknetKeyDerivation {
    /// HMAC-SHA256 based, different from Ethereum due to curve
    path_prefix: [u32; 4],
}

impl KeyDerivation for StarknetKeyDerivation {
    fn derive(&self, seed: &FieldElement, index: u32) -> FieldElement {
        // Derivation uses different path format than Ethereum
        // m/44'/60'/0'/0/0 is Ethereum, Starknet uses sequential
        let mut data = seed.to_vec();
        data.extend_from_slice(&index.to_be_bytes());
        hmac_sha256(self.path_prefix.as_bytes(), &data)
    }
}
```

### Key Storage

```rust
use aes_gcm::{aead::{Aead, KeyInit}, Aes256Gcm};

/// Encrypted key file format
struct EncryptedKeyFile {
    /// Salt for PBKDF2 (16 bytes)
    salt: [u8; 16],
    /// AES-256-GCM encrypted private key
    ciphertext: Vec<u8>,
    /// Initialization vector (12 bytes)
    nonce: [u8; 12],
    /// Authentication tag (16 bytes)
    tag: [u8; 16],
}

/// Key storage configuration
struct KeyStoreConfig {
    /// PBKDF2 iterations (minimum 100,000)
    pbkdf2_iterations: u32 = 100_000,
    /// Key file path
    path: PathBuf,
}

impl KeyStoreConfig {
    /// Encrypt private key with password
    fn encrypt(&self, private_key: &FieldElement, password: &str) -> EncryptedKeyFile {
        // 1. Generate random salt (16 bytes)
        let salt = random::<[u8; 16]>();

        // 2. Derive key from password: PBKDF2(salt, password, 100k)
        let mut key = [0u8; 32];
        pbkdf2::<HmacSha256>(password.as_bytes(), &salt, 100_000, &mut key);

        // 3. Encrypt with AES-256-GCM
        let cipher = Aes256Gcm::new_from_slice(&key).unwrap();
        let nonce = random::<[u8; 12]>();
        let ciphertext = cipher.encrypt(&nonce.into(), private_key.as_slice()).unwrap();

        // Split ciphertext into body + tag (last 16 bytes)
        let (ciphertext, tag) = ciphertext.split_at(ciphertext.len() - 16);
        let tag: [u8; 16] = tag.try_into().unwrap();

        EncryptedKeyFile { salt, ciphertext: ciphertext.to_vec(), nonce, tag }
    }

    /// Decrypt private key with password
    fn decrypt(&self, ekf: &EncryptedKeyFile, password: &str) -> FieldElement {
        let mut key = [0u8; 32];
        pbkdf2::<HmacSha256>(password.as_bytes(), &ekf.salt, 100_000, &mut key);

        let cipher = Aes256Gcm::new_from_slice(&key).unwrap();

        // Reconstruct ciphertext + tag
        let mut combined = ekf.ciphertext.clone();
        combined.extend_from_slice(&ekf.tag);

        let plaintext = cipher.decrypt(&ekf.nonce.into(), combined.as_ref()).unwrap();

        let mut fe = [0u8; 32];
        fe.copy_from_slice(&plaintext);
        fe
    }
}
```

### Signing Interface

```rust
use starknet::core::crypto::{sign, verify};

/// Signer trait for wallet operations
trait Signer {
    /// Sign a transaction
    fn sign_transaction(&self, tx: &Transfer) -> Signature;

    /// Sign an off-chain message
    fn sign_message(&self, message: &[u8]) -> Signature;

    /// Get wallet address
    fn address(&self) -> Address;
}

/// Starknet ECDSA signer
struct StarknetSigner {
    private_key: FieldElement,
    public_key: FieldElement,
    address: Address,
}

impl Signer for StarknetSigner {
    fn sign_transaction(&self, tx: &Transfer) -> Signature {
        let hash = tx.hash();
        sign(self.private_key, hash)
    }

    fn sign_message(&self, message: &[u8]) -> Signature {
        // Starknet signed message prefix
        let prefixed = format!("\x19StarkNet Message\n{}", hex::encode(message));
        let hash = starknet_keccak(prefixed.as_bytes());
        sign(self.private_key, hash)
    }

    fn address(&self) -> Address {
        self.address
    }
}

impl Transfer {
    /// Compute transaction hash for signing
    fn hash(&self) -> FieldElement {
        use starknet::core::hash::{poseidon_hash_many, StarkHash};

        let mut elements = vec![
            self.sender,
            self.receiver,
            self.token.to_field_element(),
            FieldElement::from(self.amount),
            FieldElement::from(self.nonce.0),
            self.chain_id.0,
            FieldElement::from(self.fee),
        ];

        poseidon_hash_many(&elements)
    }
}

impl Token {
    fn to_field_element(&self) -> FieldElement {
        match self {
            Token::OCTO => FieldElement::from(0_u8),
            Token::OCTO_W => FieldElement::from(1_u8),
            Token::OCTO_D => FieldElement::from(2_u8),
        }
    }
}
```

### Account Abstraction

```rust
/// Account contract interface
trait Account {
    /// Validate transaction
    fn validate(&self, caller: Address, call_data: &[FieldElement]) -> bool;

    /// Execute calls
    fn execute(&mut self, calls: Vec<Call>) -> Vec<Vec<u8>>;

    /// Get current nonce
    fn nonce(&self) -> Nonce;
}

/// OpenZeppelin-style account
struct OpenZeppelinAccount {
    public_key: FieldElement,
    nonce: Nonce,
    // ...
}

impl OpenZeppelinAccount {
    /// Validate transaction signature
    fn validate_transaction(&self, tx: &Transfer, signature: &Signature) -> bool {
        let hash = tx.hash();
        verify(self.public_key, hash, *signature)
    }
}
```

### Multi-Sig Support

```rust
/// Multi-signature wallet
struct MultisigWallet {
    threshold: u8,
    signers: Vec<Address>,
}

impl MultisigWallet {
    /// Check if threshold met
    fn is_executable(&self, valid_signatures: usize) -> bool {
        valid_signatures >= self.threshold
    }

    /// Execute if threshold met
    fn execute_if_ready(&mut self, tx: &Transfer, signatures: &[Signature]) -> Result<(), Error> {
        let valid_count = signatures.iter()
            .filter(|sig| verify(self.signers[0], tx.hash(), **sig)) // Simplified
            .count();

        if self.is_executable(valid_count) {
            // Execute transaction
            Ok(())
        } else {
            Err(Error::InsufficientSignatures)
        }
    }
}
```

### Error Handling

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum WalletError {
    #[error("Invalid private key: {0}")]
    InvalidKey(String),

    #[error("Key file not found: {0}")]
    KeyFileNotFound(PathBuf),

    #[error("Decryption failed - wrong password?")]
    DecryptionFailed,

    #[error("Insufficient signatures: got {0}, need {1}")]
    InsufficientSignatures(usize, u8),

    #[error("Transaction failed: {0}")]
    TransactionFailed(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Invalid signature")]
    InvalidSignature,
}
```

## Rationale

### Why Starknet ECDSA?

| Consideration | Starknet | Ethereum |
|--------------|----------|----------|
| Curve | Stark Curve (BLS12-381) | secp256k1 |
| Native ZK | ✅ Direct integration | ❌ Requires bridge |
| Stoolap | ✅ Same ecosystem | ⚠️ Additional complexity |
| Ecosystem | Growing | Large |

Starknet's native ZK compatibility with Stoolap is the primary driver.

### Why AES-256-GCM for Storage?

- **Authenticated encryption** - Detects tampering
- **Fast** - Hardware acceleration common
- **Standard** - Well-audited, widely used
- **Simpler than ChaCha20-Poly1305** for this use case

### Alternatives Considered

| Alternative | Pros | Cons | Rejection Reason |
|-------------|------|------|------------------|
| EVM (secp256k1) | Larger ecosystem | No native ZK | Cairo ecosystem required for Stoolap |
| MPC-based | High security | Complex | Phase 2 optimization |
| HSM | Maximum security | Expensive | Not for MVE |
| Raw key file | Simple | No password protection | Security non-negotiable |

### Trade-offs

- **Prioritized:** Starknet compatibility, ZK integration, security
- **Deprioritized:** EVM compatibility (Phase 2), advanced MPC (Phase 3)

## Implementation

### Mission 1: Core Wallet Cryptography
- Acceptance criteria:
  - [ ] KeyPair generation from random entropy
  - [ ] Key derivation (Starknet-style)
  - [ ] Transaction signing with Starknet ECDSA
  - [ ] Message signing with Starknet prefix
- Estimated complexity: Medium

### Mission 2: Secure Key Storage
- Acceptance criteria:
  - [ ] AES-256-GCM encryption/decryption
  - [ ] PBKDF2 key derivation (100k iterations)
  - [ ] Key file read/write
  - [ ] Password-protected wallet unlock
- Estimated complexity: Medium

### Mission 3: Account Interface
- Acceptance criteria:
  - [ ] OpenZeppelin account integration
  - [ ] Nonce management
  - [ ] Balance queries
  - [ ] Token transfer (OCTO-W)
- Estimated complexity: High

### Mission 4: CLI Integration
- Acceptance criteria:
  - [ ] Wallet init command
  - [ ] Balance display
  - [ ] Transfer command
  - [ ] Key file management
- Estimated complexity: Low

## Impact

### Breaking Changes
None - new functionality.

### Migration Path
- Phase 1: Single-signer accounts only
- Phase 2: Multi-sig support
- Phase 3: Hardware wallet integration

### Dependencies
- External: starknet-rs (signing, provider)
- External: aes-gcm (encryption)
- External: pbkdf2 (key derivation)

### Performance
- Signing: ~5ms per transaction
- Encryption: ~1ms for key operations
- Network: Depends on RPC provider

## Related RFCs
- RFC-0100: AI Quota Marketplace Protocol
- RFC-0101: Quota Router Agent Specification

## References
- starknet-rs: https://github.com/xJonathanLEGO/starknet-rs
- OpenZeppelin Starknet: https://github.com/OpenZeppelin/cairo-contracts
- Starknet Account Abstraction: https://docs.starknet.io/
- PBKDF2: RFC 2898
- AES-GCM: NIST SP 800-38D

---

**Submission Date:** 2026-03-03
**Last Updated:** 2026-03-03
