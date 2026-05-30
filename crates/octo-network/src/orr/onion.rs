//! Onion construction and peeling (RFC-0858 §3)
//!
//! Source-side: `construct_onion` builds layered encrypted onion (exit-first).
//! Relay-side: `peel_layer` decrypts one layer and extracts forwarding instructions.
//!
//! Uses OCrypt primitives: X25519 key exchange, HKDF-BLAKE3, ChaCha20-Poly1305.
//! All operations are deterministic given the same inputs.

use super::error::OrrError;
use super::session::{compute_hop_mac, derive_hop_nonce, derive_hop_session_key};
use super::types::{OnionHop, OnionRoute, TransportVector};

/// Next-hop instructions plaintext size (before encryption).
/// Contains: next_gateway(32) + transport_type(2) + domain_id(32) + priority(2)
///         + bandwidth_class(1) + censorship_score(1) + padding(26) = 96 bytes
pub const NEXT_HOP_INSTRUCTIONS_SIZE: usize = 96;

/// Encrypted next-hop instructions size (plaintext + 16-byte Poly1305 tag + 16 padding).
pub const ENCRYPTED_INSTRUCTIONS_SIZE: usize = 128;

/// Parameters for constructing a single onion hop during construction.
pub struct HopConstructionParams {
    /// Index of this hop (0 = entry)
    pub hop_index: u16,
    /// Public key of the relay at this hop (X25519, 32 bytes)
    pub relay_public_key: [u8; 32],
    /// Gateway ID of the relay at this hop
    pub relay_gateway_id: [u8; 32],
    /// Next-hop gateway ID (or destination for exit hop)
    pub next_gateway: [u8; 32],
    /// Transport vector for forwarding from this hop
    pub transport_vector: TransportVector,
}

/// Result of constructing a single hop layer.
struct HopLayer {
    /// The OnionHop for this layer
    hop: OnionHop,
    /// The ephemeral private key (needed to compute shared secret)
    /// Caller must zeroize after use.
    _ephemeral_private: [u8; 32],
    /// Encrypted payload for this layer (wraps all inner layers)
    encrypted_payload: Vec<u8>,
}

/// Construct an onion envelope from the source side (RFC-0858 §3.1).
///
/// The onion is built from the exit hop inward. Each layer encrypts the
/// payload and next-hop instructions for that relay.
///
/// # Arguments
/// * `route` - The onion route descriptor
/// * `hops` - Hop parameters from entry to exit (ordered: [entry, middle, ..., exit])
/// * `payload` - The plaintext payload to deliver to the destination
/// * `route_id` - The route identifier for key derivation
///
/// # Returns
/// A vector of OnionHops (one per relay) and the final layered payload blob.
///
/// # Determinism
/// Given identical inputs, produces identical output. No randomness sources
/// are used beyond deterministic key derivation. Ephemeral keys are derived
/// from BLAKE3(route_id || hop_index || "ephemeral_seed").
pub fn construct_onion(
    route: &OnionRoute,
    hops: &[HopConstructionParams],
    payload: &[u8],
    route_id: &[u8; 32],
) -> Result<(Vec<OnionHop>, Vec<u8>), OrrError> {
    if hops.is_empty() {
        return Err(OrrError::InvalidRouteCount {
            expected: route.hop_count,
            actual: 0,
        });
    }
    if hops.len() != route.hop_count as usize {
        return Err(OrrError::InvalidRouteCount {
            expected: route.hop_count,
            actual: hops.len() as u16,
        });
    }

    // Build layers from exit to entry (innermost first)
    let mut layers: Vec<HopLayer> = Vec::with_capacity(hops.len());
    let mut current_payload = payload.to_vec();

    for i in (0..hops.len()).rev() {
        let hop_params = &hops[i];
        let hop_index = i as u16;

        // 1. Generate deterministic ephemeral X25519 keypair
        let ephemeral_private = derive_ephemeral_private(route_id, hop_index);
        let ephemeral_public = x25519_derive_public(&ephemeral_private);

        // 2. Compute shared secret
        let secret = x25519_dalek::StaticSecret::from(ephemeral_private);
        let public = x25519_dalek::PublicKey::from(hop_params.relay_public_key);
        let shared_secret = crate::ocrypt::x25519_shared_secret(&secret, &public);

        // 3. Derive session key
        let session_key = derive_hop_session_key(&shared_secret, route_id, hop_index);

        // 4. Derive nonce
        let nonce = derive_hop_nonce(&session_key, route_id, hop_index);

        // 5. Build next-hop instructions (96 bytes plaintext)
        let instructions =
            build_next_hop_instructions(&hop_params.next_gateway, &hop_params.transport_vector);

        // 6. Encrypt next-hop instructions
        let encrypted_instructions =
            crate::ocrypt::encrypt(&session_key, &nonce, &instructions, &[])
                .map_err(|_e| OrrError::DecryptionFailed { hop_index })?;
        // Pad to ENCRYPTED_INSTRUCTIONS_SIZE
        let mut padded_instructions = encrypted_instructions;
        padded_instructions.resize(ENCRYPTED_INSTRUCTIONS_SIZE, 0u8);
        let encrypted_next_hop: [u8; ENCRYPTED_INSTRUCTIONS_SIZE] = padded_instructions
            .try_into()
            .map_err(|_| OrrError::DecryptionFailed { hop_index })?;

        // 7. Encrypt the current payload (which wraps all inner layers)
        let encrypted_payload = crate::ocrypt::encrypt(&session_key, &nonce, &current_payload, &[])
            .map_err(|_e| OrrError::DecryptionFailed { hop_index })?;

        // 8. Compute hop MAC
        let hop_mac = compute_hop_mac(&session_key, &encrypted_payload, &encrypted_next_hop);

        // 9. Assemble the OnionHop
        let hop = OnionHop {
            hop_index,
            relay_gateway: hop_params.relay_gateway_id,
            transport_vector_root: compute_transport_vector_root(&hop_params.transport_vector),
            encrypted_next_hop,
            encrypted_payload_fragment: Vec::new(), // Payload is in the layered blob
            hop_mac,
            ephemeral_public_key: ephemeral_public,
        };

        layers.push(HopLayer {
            hop,
            _ephemeral_private: ephemeral_private,
            encrypted_payload,
        });

        // The encrypted payload becomes the payload for the next outer layer
        current_payload = layers.last().unwrap().encrypted_payload.clone();
    }

    // Assemble: hops go from entry (outer) to exit (inner)
    // layers is built exit-first, so reverse for output
    layers.reverse();
    let onion_hops: Vec<OnionHop> = layers.into_iter().map(|l| l.hop).collect();
    let layered_payload = current_payload;

    Ok((onion_hops, layered_payload))
}

/// Peel one layer of an onion at a relay (RFC-0858 §3.2).
///
/// The relay uses its private key to decrypt the current hop's instructions
/// and extract the next-hop forwarding information plus the inner onion payload.
///
/// # Arguments
/// * `hop` - The current OnionHop for this relay
/// * `relay_private_key` - This relay's X25519 private key (32 bytes)
/// * `route_id` - The route identifier for key derivation
/// * `layered_payload` - The current layered payload blob
///
/// # Returns
/// A `PeeledLayer` containing the next-hop gateway, transport instructions,
/// and the inner payload to forward.
pub fn peel_layer(
    hop: &OnionHop,
    relay_private_key: &[u8; 32],
    route_id: &[u8; 32],
    layered_payload: &[u8],
) -> Result<PeeledLayer, OrrError> {
    let hop_index = hop.hop_index;

    // 1. Compute shared secret = X25519(relay_private, ephemeral_public)
    let secret = x25519_dalek::StaticSecret::from(*relay_private_key);
    let public = x25519_dalek::PublicKey::from(hop.ephemeral_public_key);
    let shared_secret = crate::ocrypt::x25519_shared_secret(&secret, &public);

    // 2. Derive session key
    let session_key = derive_hop_session_key(&shared_secret, route_id, hop_index);

    // 3. Derive nonce
    let nonce = derive_hop_nonce(&session_key, route_id, hop_index);

    // 4. Verify hop MAC
    let expected_mac = compute_hop_mac(&session_key, layered_payload, &hop.encrypted_next_hop);
    if expected_mac != hop.hop_mac {
        return Err(OrrError::MacVerificationFailed { hop_index });
    }

    // 5. Decrypt next-hop instructions
    // encrypted_next_hop is 128 bytes: [96 ciphertext + 16 tag + 16 padding]
    // Pass only the ciphertext+tag portion (112 bytes) to the AEAD decrypt
    let instructions_bytes = crate::ocrypt::decrypt(
        &session_key,
        &nonce,
        &hop.encrypted_next_hop[..96 + crate::ocrypt::TAG_SIZE],
        &[],
    )
    .map_err(|_| OrrError::DecryptionFailed { hop_index })?;

    // 6. Parse next-hop instructions
    let (next_gateway, transport) = parse_next_hop_instructions(&instructions_bytes)?;

    // 7. Decrypt inner payload
    let inner_payload = crate::ocrypt::decrypt(&session_key, &nonce, layered_payload, &[])
        .map_err(|_| OrrError::DecryptionFailed { hop_index })?;

    Ok(PeeledLayer {
        next_gateway,
        transport,
        inner_payload,
        hop_index,
    })
}

/// Result of peeling one onion layer.
#[derive(Debug, Clone)]
pub struct PeeledLayer {
    /// Next-hop gateway identifier
    pub next_gateway: [u8; 32],
    /// Transport instructions for forwarding
    pub transport: TransportVector,
    /// Inner payload (to be forwarded to next hop)
    pub inner_payload: Vec<u8>,
    /// Which hop index was just peeled
    pub hop_index: u16,
}

/// Build next-hop instructions (96 bytes plaintext).
///
/// Format:
///   [0..32]  next_gateway (32 bytes)
///   [32..34] transport_type (2 bytes, big-endian)
///   [34..66] domain_id (32 bytes)
///   [66..68] priority (2 bytes, big-endian)
///   [68..69] bandwidth_class (1 byte)
///   [69..70] censorship_score (1 byte)
///   [70..96] reserved/padding (26 bytes, zero)
fn build_next_hop_instructions(next_gateway: &[u8; 32], transport: &TransportVector) -> Vec<u8> {
    let mut buf = Vec::with_capacity(NEXT_HOP_INSTRUCTIONS_SIZE);
    buf.extend_from_slice(next_gateway);
    buf.extend_from_slice(&transport.transport_type.to_be_bytes());
    buf.extend_from_slice(&transport.domain_id);
    buf.extend_from_slice(&transport.priority.to_be_bytes());
    buf.push(transport.bandwidth_class);
    buf.push(transport.censorship_score);
    buf.resize(NEXT_HOP_INSTRUCTIONS_SIZE, 0); // padding
    buf
}

/// Parse next-hop instructions from decrypted plaintext.
fn parse_next_hop_instructions(data: &[u8]) -> Result<([u8; 32], TransportVector), OrrError> {
    if data.len() < NEXT_HOP_INSTRUCTIONS_SIZE {
        return Err(OrrError::DecryptionFailed { hop_index: 0 });
    }

    let mut next_gateway = [0u8; 32];
    next_gateway.copy_from_slice(&data[0..32]);

    let transport_type = u16::from_be_bytes([data[32], data[33]]);
    let mut domain_id = [0u8; 32];
    domain_id.copy_from_slice(&data[34..66]);
    let priority = u16::from_be_bytes([data[66], data[67]]);
    let bandwidth_class = data[68];
    let censorship_score = data[69];

    Ok((
        next_gateway,
        TransportVector {
            transport_type,
            domain_id,
            priority,
            bandwidth_class,
            censorship_score,
        },
    ))
}

/// Derive a deterministic ephemeral private key for onion construction.
///
/// ephemeral_private = BLAKE3-256(route_id || hop_index || "orr:ephemeral:v1")
///
/// This ensures the same route always produces the same ephemeral keys,
/// enabling deterministic replay for consensus verification.
fn derive_ephemeral_private(route_id: &[u8; 32], hop_index: u16) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(route_id);
    hasher.update(&hop_index.to_be_bytes());
    hasher.update(b"orr:ephemeral:v1");
    *hasher.finalize().as_bytes()
}

/// Derive the public key from a private key (X25519 scalar multiplication with basepoint).
fn x25519_derive_public(private_key: &[u8; 32]) -> [u8; 32] {
    let secret = x25519_dalek::StaticSecret::from(*private_key);
    let public = x25519_dalek::PublicKey::from(&secret);
    public.to_bytes()
}

/// Compute the transport vector root for inclusion in the OnionHop.
///
/// BLAKE3-256(transport_type || domain_id || priority || bandwidth_class || censorship_score)
fn compute_transport_vector_root(tv: &TransportVector) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&tv.transport_type.to_be_bytes());
    hasher.update(&tv.domain_id);
    hasher.update(&tv.priority.to_be_bytes());
    hasher.update(&[tv.bandwidth_class]);
    hasher.update(&[tv.censorship_score]);
    *hasher.finalize().as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_hop_params_from_private(
        index: u16,
        relay_private: u8,
        next_id: u8,
    ) -> (HopConstructionParams, [u8; 32]) {
        // Derive the correct public key from the private key
        let priv_bytes = [relay_private; 32];
        let secret = x25519_dalek::StaticSecret::from(priv_bytes);
        let public = x25519_dalek::PublicKey::from(&secret);
        let pub_bytes = public.to_bytes();
        (
            HopConstructionParams {
                hop_index: index,
                relay_public_key: pub_bytes,
                relay_gateway_id: [relay_private; 32],
                next_gateway: [next_id; 32],
                transport_vector: TransportVector {
                    transport_type: 0x0001,
                    domain_id: [0xAA; 32],
                    priority: 1,
                    bandwidth_class: 100,
                    censorship_score: 50,
                },
            },
            priv_bytes,
        )
    }

    fn make_route(hop_count: u16) -> OnionRoute {
        OnionRoute {
            route_id: [0x42; 32],
            mission_id: [0u8; 32],
            route_epoch: 100,
            hop_count,
            entry_gateway: [0x01; 32],
            exit_gateway: [0x03; 32],
            layered_route_root: [0u8; 32],
            construction_timestamp: 500,
            flags: 0,
        }
    }

    #[test]
    fn test_construct_single_hop() {
        let route = make_route(1);
        let (hop, _) = make_hop_params_from_private(0, 0x01, 0xFF);
        let hops = vec![hop];
        let payload = b"hello destination";
        let route_id = route.route_id;

        let result = construct_onion(&route, &hops, payload, &route_id);
        assert!(result.is_ok());
        let (onion_hops, layered_payload) = result.unwrap();
        assert_eq!(onion_hops.len(), 1);
        assert!(!layered_payload.is_empty());
    }

    #[test]
    fn test_construct_three_hop() {
        let route = make_route(3);
        let (h0, _) = make_hop_params_from_private(0, 0x01, 0x02);
        let (h1, _) = make_hop_params_from_private(1, 0x02, 0x03);
        let (h2, _) = make_hop_params_from_private(2, 0x03, 0xFF);
        let hops = vec![h0, h1, h2];
        let payload = b"secret message for multi-hop onion";
        let route_id = route.route_id;

        let result = construct_onion(&route, &hops, payload, &route_id);
        assert!(result.is_ok());
        let (onion_hops, layered_payload) = result.unwrap();
        assert_eq!(onion_hops.len(), 3);
        assert!(!layered_payload.is_empty());
        assert!(layered_payload.len() > payload.len());
    }

    #[test]
    fn test_construct_deterministic() {
        let route = make_route(2);
        let (h0, _) = make_hop_params_from_private(0, 0x01, 0x02);
        let (h1, _) = make_hop_params_from_private(1, 0x02, 0xFF);
        let hops = vec![h0, h1];
        let payload = b"determinism test";
        let route_id = route.route_id;

        let r1 = construct_onion(&route, &hops, payload, &route_id).unwrap();
        let r2 = construct_onion(&route, &hops, payload, &route_id).unwrap();
        assert_eq!(r1.0.len(), r2.0.len());
        assert_eq!(r1.0[0].ephemeral_public_key, r2.0[0].ephemeral_public_key);
        assert_eq!(r1.0[1].ephemeral_public_key, r2.0[1].ephemeral_public_key);
        assert_eq!(r1.1, r2.1);
    }

    #[test]
    fn test_construct_empty_hops() {
        let route = make_route(1);
        let hops = vec![];
        let payload = b"test";

        let result = construct_onion(&route, &hops, payload, &route.route_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_construct_hop_count_mismatch() {
        let route = make_route(3);
        let (hop, _) = make_hop_params_from_private(0, 0x01, 0x02);
        let hops = vec![hop]; // Only 1 hop, expects 3
        let payload = b"test";

        let result = construct_onion(&route, &hops, payload, &route.route_id);
        assert!(matches!(
            result,
            Err(OrrError::InvalidRouteCount {
                expected: 3,
                actual: 1
            })
        ));
    }

    #[test]
    fn test_peel_single_hop() {
        let route = make_route(1);
        let (hop, relay_private) = make_hop_params_from_private(0, 0x01, 0xFF);
        let hops = vec![hop];
        let payload = b"peel test payload";
        let route_id = route.route_id;

        let (onion_hops, layered_payload) =
            construct_onion(&route, &hops, payload, &route_id).unwrap();

        let peeled = peel_layer(&onion_hops[0], &relay_private, &route_id, &layered_payload);
        assert!(peeled.is_ok());
        let layer = peeled.unwrap();
        assert_eq!(layer.next_gateway, [0xFF; 32]);
        assert_eq!(layer.inner_payload, payload);
    }

    #[test]
    fn test_peel_wrong_key_fails() {
        let route = make_route(1);
        let (hop, _) = make_hop_params_from_private(0, 0x01, 0xFF);
        let wrong_private = [0x99u8; 32]; // Wrong private key
        let hops = vec![hop];
        let payload = b"wrong key test";
        let route_id = route.route_id;

        let (onion_hops, layered_payload) =
            construct_onion(&route, &hops, payload, &route_id).unwrap();

        let result = peel_layer(&onion_hops[0], &wrong_private, &route_id, &layered_payload);
        assert!(result.is_err());
    }

    #[test]
    fn test_full_onion_pipeline() {
        let route = make_route(3);
        let (h0, p0) = make_hop_params_from_private(0, 0x01, 0x02);
        let (h1, p1) = make_hop_params_from_private(1, 0x02, 0x03);
        let (h2, p2) = make_hop_params_from_private(2, 0x03, 0xFF);
        let hops = vec![h0, h1, h2];
        let payload = b"full pipeline test with three hops";
        let route_id = route.route_id;

        let (onion_hops, mut current_payload) =
            construct_onion(&route, &hops, payload, &route_id).unwrap();

        // Peel at entry relay (hop 0)
        let peeled_entry = peel_layer(&onion_hops[0], &p0, &route_id, &current_payload).unwrap();
        assert_eq!(peeled_entry.next_gateway, [0x02; 32]);
        current_payload = peeled_entry.inner_payload;

        // Peel at middle relay (hop 1)
        let peeled_middle = peel_layer(&onion_hops[1], &p1, &route_id, &current_payload).unwrap();
        assert_eq!(peeled_middle.next_gateway, [0x03; 32]);
        current_payload = peeled_middle.inner_payload;

        // Peel at exit relay (hop 2)
        let peeled_exit = peel_layer(&onion_hops[2], &p2, &route_id, &current_payload).unwrap();
        assert_eq!(peeled_exit.next_gateway, [0xFF; 32]);

        // Final payload should match original
        assert_eq!(peeled_exit.inner_payload, payload);
    }

    #[test]
    fn test_build_and_parse_instructions_roundtrip() {
        let next_gw = [0xAB; 32];
        let tv = TransportVector {
            transport_type: 0x0002,
            domain_id: [0xCD; 32],
            priority: 42,
            bandwidth_class: 200,
            censorship_score: 150,
        };
        let instructions = build_next_hop_instructions(&next_gw, &tv);
        assert_eq!(instructions.len(), NEXT_HOP_INSTRUCTIONS_SIZE);

        let (parsed_gw, parsed_tv) = parse_next_hop_instructions(&instructions).unwrap();
        assert_eq!(parsed_gw, next_gw);
        assert_eq!(parsed_tv.transport_type, tv.transport_type);
        assert_eq!(parsed_tv.domain_id, tv.domain_id);
        assert_eq!(parsed_tv.priority, tv.priority);
        assert_eq!(parsed_tv.bandwidth_class, tv.bandwidth_class);
        assert_eq!(parsed_tv.censorship_score, tv.censorship_score);
    }

    #[test]
    fn test_ephemeral_key_derivation_deterministic() {
        let route_id = [0x42; 32];
        let k1 = derive_ephemeral_private(&route_id, 0);
        let k2 = derive_ephemeral_private(&route_id, 0);
        assert_eq!(k1, k2);

        let k3 = derive_ephemeral_private(&route_id, 1);
        assert_ne!(k1, k3);
    }
}
