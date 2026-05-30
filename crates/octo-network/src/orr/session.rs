//! Session key derivation for ORR (RFC-0858 §4)

/// Derive per-hop session key (aligned with RFC-0858 §4.1)
/// HKDF-BLAKE3(salt="orr:hop_session:v1", ikm=session_key, info=route_id||hop_index)
pub fn derive_hop_session_key(
    shared_secret: &[u8; 32],
    route_id: &[u8; 32],
    hop_index: u16,
) -> [u8; 32] {
    let salt = b"orr:hop_session:v1";
    let mut info = Vec::with_capacity(34);
    info.extend_from_slice(route_id);
    info.extend_from_slice(&hop_index.to_be_bytes());
    let mut key = [0u8; 32];
    crate::ocrypt::hkdf_blake3(salt, shared_secret, &info, &mut key);
    key
}

/// Derive nonce for ChaCha20-Poly1305 (12 bytes)
/// HKDF-BLAKE3(salt="orr:hop_nonce:v1", ikm=session_key, info=route_id||hop_index)
/// Per-route key isolation per RFC-0858 §4.2
pub fn derive_hop_nonce(session_key: &[u8; 32], route_id: &[u8; 32], hop_index: u16) -> [u8; 12] {
    let salt = b"orr:hop_nonce:v1";
    let mut info = Vec::with_capacity(34);
    info.extend_from_slice(route_id);
    info.extend_from_slice(&hop_index.to_be_bytes());
    let mut full = [0u8; 32];
    crate::ocrypt::hkdf_blake3(salt, session_key, &info, &mut full);
    let mut nonce = [0u8; 12];
    nonce.copy_from_slice(&full[..12]);
    nonce
}

/// Compute hop MAC: BLAKE3-256("orr:hop_mac:v1" || session_key || encrypted_fragment || encrypted_instructions)
pub fn compute_hop_mac(
    session_key: &[u8; 32],
    encrypted_fragment: &[u8],
    encrypted_instructions: &[u8],
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"orr:hop_mac:v1");
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
        let route = [0xBB; 32];
        let k1 = derive_hop_session_key(&secret, &route, 0);
        let k2 = derive_hop_session_key(&secret, &route, 0);
        assert_eq!(k1, k2);
    }

    #[test]
    fn test_session_key_different_hops() {
        let secret = [0xAA; 32];
        let route = [0xBB; 32];
        let k0 = derive_hop_session_key(&secret, &route, 0);
        let k1 = derive_hop_session_key(&secret, &route, 1);
        assert_ne!(k0, k1);
    }

    #[test]
    fn test_session_key_different_routes() {
        let secret = [0xAA; 32];
        let route_a = [0xBB; 32];
        let route_b = [0xCC; 32];
        let k1 = derive_hop_session_key(&secret, &route_a, 0);
        let k2 = derive_hop_session_key(&secret, &route_b, 0);
        assert_ne!(k1, k2);
    }

    #[test]
    fn test_nonce_deterministic() {
        let key = [0xAA; 32];
        let route = [0xBB; 32];
        let n1 = derive_hop_nonce(&key, &route, 0);
        let n2 = derive_hop_nonce(&key, &route, 0);
        assert_eq!(n1, n2);
    }

    #[test]
    fn test_nonce_length() {
        let key = [0xAA; 32];
        let route = [0xBB; 32];
        let nonce = derive_hop_nonce(&key, &route, 5);
        assert_eq!(nonce.len(), 12);
    }

    #[test]
    fn test_nonce_different_hops() {
        let key = [0xAA; 32];
        let route = [0xBB; 32];
        let n0 = derive_hop_nonce(&key, &route, 0);
        let n1 = derive_hop_nonce(&key, &route, 1);
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
}
