# Research: Wallet Cryptography for CipherOcto

## Executive Summary

This research investigates the cryptographic foundations required for wallet implementation in the CipherOcto AI Quota Marketplace. Focuses on cryptographic primitives, signature schemes, and key management compatible with the Cairo ecosystem (Starknet) used by Stoolap for ZK proofs.

## Problem Statement

CipherOcto needs cryptographic wallet infrastructure that:
1. Provides secure key management and signing
2. Integrates with Cairo/Starknet ecosystem
3. Supports token transactions (OCTO-W, OCTO-D, OCTO)
4. Enables ZK proof verification for Stoolap integration

## Research Scope

- **Included:** Cryptographic primitives, signature schemes, key derivation, account models
- **Excluded:** User-facing wallet applications (consumer UX) - separate research

---

## Cryptographic Foundations

### 1. Starknet Signature Scheme

Starknet uses a different signature scheme from Ethereum's ECDSA:

| Aspect | Ethereum | Starknet |
|--------|----------|----------|
| **Curve** | secp256k1 | Stark Curve (EC over BLS12-381) |
| **Signature** | ECDSA | StarkNet ECDSA |
| **Key Size** | 32 bytes | 32 bytes |
| **Address** | 20 bytes | 32 bytes |

```rust
// Starknet key pair using starknet-rs
use starknet::core::crypto::{sign, verify};
use starknet::core::types::FieldElement;

// Private key → Public key → Address
let private_key = FieldElement::from_hex_be("0x...").unwrap();
let public_key = private_key_to_public_key(private_key);
let address = compute_address(public_key);
```

### 2. Account Model

Starknet uses account abstraction - every account is a smart contract:

```cairo
// Minimal Account Contract (OpenZeppelin)
#[starknet::contract]
mod Account {
    #[storage]
    struct Storage {
        public_key: felt252,
    }

    #[external]
    fn __validate__(calldata: Array<felt252>) -> felt252 {
        // Verify signature matches public_key
    }

    #[external]
    fn __execute__(calls: Array<Call>) -> Array<Span<felt252>> {
        // Execute calls if __validate__ passed
    }
}
```

**Account Types:**

| Type | Validation | Use Case |
|------|-----------|----------|
| **Argent** | Multi-party computation | Mobile wallets |
| **Braavos** | Hardware security | High security |
| **Generic** | Single ECDSA | Standard accounts |
| **Multisig** | M-of-N signatures | Treasury |

### 3. Key Derivation

```rust
// BIP-32 style derivation for Starknet
// Note: Starknet uses different path format

// Standard derivation path: m/44'/60'/0'/0/0 (Ethereum)
// Starknet: No BIP-44 yet, use sequential nonces

struct KeyDerivation {
    seed: [u8; 32],
    path: Vec<u32>,
}

impl KeyDerivation {
    fn derive(&self, index: u32) -> FieldElement {
        // HMAC-SHA256 based derivation
        // Different from Ethereum due to curve difference
    }
}
```

### 4. Local Key Storage

For CLI tools, keys must be stored securely locally:

```rust
struct SecureKeyStore {
    path: PathBuf,
    // Encryption: AES-256-GCM with key derived from user password
}

impl SecureKeyStore {
    fn encrypt_key(&self, private_key: FieldElement, password: &str) -> Vec<u8> {
        // PBKDF2 (100,000 iterations) → AES-256-GCM
    }

    fn decrypt_key(&self, encrypted: &[u8], password: &str) -> FieldElement {
        // Reverse the process
    }
}
```

**Storage Options:**

| Method | Security | Use Case |
|--------|----------|----------|
| **Encrypted file** | Medium | CLI tools |
| **OS Keychain** | High | Desktop apps |
| **HSM** | Very High | Production |
| **MPC** | Very High | Institutional |

---

## Cryptographic Operations

### 1. Transaction Signing

```rust
struct OctoTransaction {
    sender: FieldElement,
    receiver: FieldElement,
    token: TokenType,  // OCTO, OCTO-W, OCTO-D
    amount: u64,
    nonce: u64,
    chain_id: FieldElement,
}

impl OctoTransaction {
    fn hash(&self) -> FieldElement {
        // Poseidon hash of transaction fields
    }

    fn sign(&self, private_key: FieldElement) -> Signature {
        // StarkNet ECDSA sign
        sign(private_key, self.hash())
    }
}
```

### 2. Message Signing (Off-chain)

```rust
// Sign messages for off-chain authentication
fn sign_message(private_key: FieldElement, message: &[u8]) -> (FieldElement, FieldElement) {
    // Starknet signed message prefix: "\x19StarkNet Message\n"
    let prefixed = format!("\x19StarkNet Message\n{}", hex::encode(message));
    let hash = starknet_hash(prefixed.as_bytes());
    sign(private_key, hash)
}
```

### 3. Multi-Sig (Threshold Signatures)

```rust
struct MultisigWallet {
    threshold: u8,
    signers: Vec<FieldElement>,
}

impl MultisigWallet {
    // Collect signatures, execute when threshold reached
    fn execute_if_ready(&mut self, tx: &Transaction, signatures: Vec<Signature>) -> bool {
        let valid = signatures.iter()
            .filter(|sig| verify(self.signers.clone(), tx.hash(), *sig))
            .count();

        if valid >= self.threshold {
            self.execute(tx)
        } else {
            false
        }
    }
}
```

---

## Stoolap ZK Integration

### Proof Verification

```rust
// Verify Stoolap execution proof
use starknet::core::types::TransactionReceipt;

struct ProofVerifier {
    stoolap_contract: FieldElement,
}

impl ProofVerifier {
    async fn verify_execution_proof(
        &self,
        provider: &Provider,
        proof: &HexaryProof,
    ) -> Result<bool> {
        // Call Stoolap verifier contract
        let call = FunctionCall {
            contract_address: self.stoolap_contract,
            entry_point_selector: 0x1234, // verify_proof
            calldata: proof.to_calldata(),
        };

        provider.call(call).await
    }
}
```

### ZK-Friendly Operations

| Operation | ZK-Friendly | Notes |
|-----------|-------------|-------|
| Balance transfer | ✅ | Standard ERC-20 |
| Multi-sig | ✅ | Threshold sigs |
| Confidential txs | ⚠️ | Requires commitment schemes |
| Proof verification | ✅ | Native to Starknet |

---

## Recommended Implementation

### For MVE: Direct Starknet Integration

```rust
// Minimal wallet for CLI tool
use starknet::providers::Provider;
use starknet::signers::Signer;

struct QuotaWallet<P: Provider> {
    provider: P,
    account: LocalWallet,
}

impl<P: Provider> QuotaWallet<P> {
    // Initialize from encrypted key file
    async fn from_keyfile(path: &Path, password: &str) -> Result<Self> {
        let encrypted = std::fs::read(path)?;
        let private_key = decrypt_key(encrypted, password)?;
        let account = LocalWallet::from_key(private_key);
        Ok(Self { provider, account })
    }

    // Pay for quota request
    async fn pay_for_quota(&self, to: FieldElement, amount: u64) -> Result<FieldElement> {
        let tx = TransactionRequest {
            to,
            amount,
            // ... token transfer calldata
        };
        self.provider.broadcast_tx(tx, &self.account).await
    }
}
```

### Key Components

| Component | Technology | Purpose |
|-----------|------------|---------|
| **Signing** | starknet-rs | Transaction signing |
| **Key Storage** | AES-256-GCM | Local encrypted storage |
| **Provider** | starknet-rs JSON-RPC | Network communication |
| **Account** | OpenZeppelin | Smart contract wallet |

---

## Risk Assessment

| Risk | Mitigation | Severity |
|------|------------|----------|
| Private key exposure | Use OS keychain, never log keys | Critical |
| Signature replay | Include nonce + chain_id in every tx | High |
| Curve vulnerability | Use starknet-rs (audited) | Low |
| MPC complexity | Defer to Phase 2 | Medium |

---

## Recommendations

### Phase 1 (MVE)
- Use starknet-rs for signing and provider
- Encrypted keyfile with password protection
- Simple single-signer account
- Manual nonce management

### Phase 2
- Add multi-sig support for governance
- Integrate OS keychain (macOS Keychain, Windows Credential Manager)
- Hardware wallet signing (Ledger via HID)

### Phase 3
- MPC-based key sharding
- Threshold signatures for treasury
- ZK-based confidential transactions

---

## Next Steps

- [ ] Draft RFC for Wallet Cryptography Specification
- [ ] Define KeyDerivation trait for extensibility
- [ ] Create Use Case for CLI wallet integration

---

## References

- Parent Document: BLUEPRINT.md
- Stoolap: `/home/mmacedoeu/_w/databases/stoolap`
- starknet-rs: https://github.com/xJonathanLEGO/starknet-rs
- OpenZeppelin Starknet Accounts: https://github.com/OpenZeppelin/cairo-contracts
- Starknet ECDSA: https://docs.starknet.io/

---

**Research Status:** Complete (Cryptography Focus)
**Recommended Action:** Proceed to RFC for Wallet Cryptography
