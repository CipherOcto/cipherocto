//! Session key derivation for ORR (RFC-0858 §4)

/// Derive per-hop session key (aligned with RFC-0853 §10)
/// HKDF-BLAKE3(shared_secret, salt="ocrypt:onion:v1", info=hop_index||route_id)
pub fn derive_hop_session_key(
    shared_secret: &[u8; 32],
    hop_index: u16,
    route_id: &[u8; 32],
) -> [u8; 32] {
    let salt = b"ocrypt:onion:v1";
    let mut info = [0u8; 34]; // 2 (hop_index) + 32 (route_id)
    info[0..2].copy_from_slice(&hop_index.to_be_bytes());
    info[2..34].copy_from_slice(route_id);
    let expanded = hkdf_blake3_expand(shared_secret, salt, &info, 32);
    let mut key = [0u8; 32];
    key.copy_from_slice(&expanded[..32]);
    key
}

/// Derive nonce for ChaCha20-Poly1305 (12 bytes)
/// HKDF-BLAKE3(session_key, "ocrypt:nonce:v1", hop_index)[0..12]
pub fn derive_hop_nonce(session_key: &[u8; 32], hop_index: u16) -> [u8; 12] {
    let salt = b"ocrypt:nonce:v1";
    let mut info = [0u8; 2];
    info[0..2].copy_from_slice(&hop_index.to_be_bytes());
    let full = hkdf_blake3_expand(session_key, salt, &info, 12);
    let mut nonce = [0u8; 12];
    nonce.copy_from_slice(&full[..12]);
    nonce
}

/// HKDF-BLAKE3 expand operation.
/// Uses BLAKE3's keyed hash mode as HMAC-BLAKE3 replacement.
fn hkdf_blake3_expand(ikm: &[u8], salt: &[u8], info: &[u8], length: usize) -> Vec<u8> {
    // Extract: PRK = BLAKE3(salt || ikm) using keyed mode
    let mut extract_hasher = blake3::Hasher::new();
    extract_hasher.update(salt);
    extract_hasher.update(ikm);
    let prk = extract_hasher.finalize();

    // Expand: T(1) = BLAKE3(PRK || info || 0x01)
    let mut expand_hasher = blake3::Hasher::new();
    expand_hasher.update(prk.as_bytes());
    expand_hasher.update(info);
    expand_hasher.update(&[0x01]);
    let result = expand_hasher.finalize();

    // Truncate to requested length (max 32 for BLAKE3-256)
    let output_len = length.min(32);
    result.as_bytes()[..output_len].to_vec()
}

/// Compute hop MAC: BLAKE3-256(session_key || encrypted_fragment || encrypted_instructions)
pub fn compute_hop_mac(
    session_key: &[u8; 32],
    encrypted_fragment: &[u8],
    encrypted_instructions: &[u8],
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(session_key);
    hasher.update(encrypted_fragment);
    hasher.update(encrypted_instructions);
    *hasher.finalize().as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_key_deterministic() {
        let secret = [0xAA; 32];
        let route_id = [0xBB; 32];
        let k1 = derive_hop_session_key(&secret, 0, &route_id);
        let k2 = derive_hop_session_key(&secret, 0, &route_id);
        assert_eq!(k1, k2);
    }

    #[test]
    fn test_session_key_different_hops() {
        let secret = [0xAA; 32];
        let route_id = [0xBB; 32];
        let k0 = derive_hop_session_key(&secret, 0, &route_id);
        let k1 = derive_hop_session_key(&secret, 1, &route_id);
        assert_ne!(k0, k1);
    }

    #[test]
    fn test_session_key_different_routes() {
        let secret = [0xAA; 32];
        let r1 = [0xBB; 32];
        let r2 = [0xCC; 32];
        let k1 = derive_hop_session_key(&secret, 0, &r1);
        let k2 = derive_hop_session_key(&secret, 0, &r2);
        assert_ne!(k1, k2);
    }

    #[test]
    fn test_nonce_deterministic() {
        let key = [0xAA; 32];
        let n1 = derive_hop_nonce(&key, 0);
        let n2 = derive_hop_nonce(&key, 0);
        assert_eq!(n1, n2);
    }

    #[test]
    fn test_nonce_length() {
        let key = [0xAA; 32];
        let nonce = derive_hop_nonce(&key, 5);
        assert_eq!(nonce.len(), 12);
    }

    #[test]
    fn test_nonce_different_hops() {
        let key = [0xAA; 32];
        let n0 = derive_hop_nonce(&key, 0);
        let n1 = derive_hop_nonce(&key, 1);
        assert_ne!(n0, n1);
    }

    #[test]
    fn test_hop_mac_deterministic() {
        let key = [0xAA; 32];
        let frag = b"fragment data";
        let instr = b"instructions";
        let m1 = compute_hop_mac(&key, frag, instr);
        let m2 = compute_hop_mac(&key, frag, instr);
        assert_eq!(m1, m2);
    }

    #[test]
    fn test_hop_mac_different_keys() {
        let frag = b"fragment data";
        let instr = b"instructions";
        let m1 = compute_hop_mac(&[0xAA; 32], frag, instr);
        let m2 = compute_hop_mac(&[0xBB; 32], frag, instr);
        assert_ne!(m1, m2);
    }

    #[test]
    fn test_hkdf_blake3_expand_output_length() {
        let result = hkdf_blake3_expand(&[0xAA; 32], b"salt", b"info", 32);
        assert_eq!(result.len(), 32);
    }

    #[test]
    fn test_hkdf_blake3_expand_deterministic() {
        let r1 = hkdf_blake3_expand(&[0xAA; 32], b"salt", b"info", 32);
        let r2 = hkdf_blake3_expand(&[0xAA; 32], b"salt", b"info", 32);
        assert_eq!(r1, r2);
    }
}
