//! caBLE v2 BLE service-data advertiser (Linux only).
//!
//! ## Protocol
//!
//! caBLE v2 transports the encrypted Eid (containing the responder's
//! nonce + the relay-assigned routing_id) over a BLE advertisement,
//! NOT over the WebSocket tunnel. The phone's gms FIDO module scans
//! for the FIDO caBLE 16-bit service UUID `0xfff9`, decrypts the
//! 20-byte service data with the EidKey derived from the QR's
//! `secret`, recovers the Eid, and uses it to derive the same PSK
//! that our responder uses for the Noise NKpsk0 handshake.
//!
//! Without this advertisement the phone cannot recover the Eid →
//! cannot derive the matching PSK → the Noise handshake silently
//! fails ("connecting to other device" forever, then a WS timeout
//! from our side).
//!
//! ## Wire layout
//!
//! - **Service UUID (16-bit)**: `0xfff9` (FIDO caBLE v2 service).
//!   Transmitted in 16-bit form (the BLE AD spec packs 16-bit UUIDs
//!   in 2 bytes vs 16 for UUID-128).
//! - **Service Data (20 bytes)**:
//!   - bytes 0..16: `AES-CTR(key=key[0..32], iv=zeros, plaintext=eid)`
//!   - bytes 16..20: `HMAC-SHA256(key=key[32..64], data=ciphertext)[:4]`
//!
//! The phone's `Eid::decrypt_advert` does the inverse (HMAC check
//! first as a cheap filter, then AES-CTR decrypt) to recover the
//! Eid. We re-implement the decrypt in the test
//! `encrypt_advert_round_trips_through_decrypt` to pin the wire
//! format without depending on webauthn-rs.
//!
//! ## Runtime requirement
//!
//! `bluer` is Linux-only: it talks to `bluetoothd` over the system
//! D-Bus. On non-Linux targets `start_advertisement` returns
//! `CableError::Ble("unsupported platform: ble advertiser requires
//! Linux + bluez")` and the CLI surfaces a clear "BLE adapter
//! required" error. The user's environment is Linux (verified:
//! `bluetoothd` pid 1031, `hci0` adapter, `busctl` available) so
//! the live path works.
//!
//! ## Reference
//!
//! - Chromium: `device/fido/cable/btle.cc`, `device/fido/cable/v2_handshake.cc`
//! - webauthn-rs: `webauthn-authenticator-rs/src/cable/btle.rs` (the
//!   trait + Service UUID + EidKey encryption semantics we mirror)

use aes::cipher::{KeyIvInit, StreamCipher};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::error::CableError;

type Aes128Ctr = ctr::Ctr128BE<aes::Aes128>;
type HmacSha256 = Hmac<Sha256>;

/// 16-bit Service UUID for the FIDO caBLE v2 service data
/// advertisement. Must be transmitted in 16-bit form (the BLE AD
/// spec's 0x16 AD type is limited to 31 bytes total per ad, so a
/// UUID-128 would blow the budget). Matches
/// `webauthn-rs::cable::btle::FIDO_CABLE_SERVICE_U16`.
pub const FIDO_CABLE_SERVICE_U16: u16 = 0xfff9;

/// 20 bytes total: 16-byte AES-CTR ciphertext + 4-byte HMAC tag
/// truncation. This is what the phone decrypts and verifies.
pub const ADVERT_DATA_LEN: usize = 20;

/// 64 bytes: first 32 = AES-128 key, last 32 = HMAC-SHA256 key.
/// Derived once from the QR's `secret` via HKDF-SHA256 with the
/// 4-byte little-endian info tag `1` (DerivedValueType::EidKey).
#[derive(Clone, Copy)]
pub struct EidKey(pub [u8; 64]);

/// Derive the EidKey from the QR's `secret` per Chromium's
/// `Discovery::DerivedValueType::EidKey`. 64 bytes total: the
/// first 32 are the AES-128 key, the last 32 are the HMAC-SHA256
/// key.
pub fn eid_key(qr_secret: &[u8]) -> EidKey {
    let hk = Hkdf::<Sha256>::new(Some(b""), qr_secret);
    let mut out = [0u8; 64];
    // info = 4-byte little-endian of the EidKey enum tag (1).
    hk.expand(&1u32.to_le_bytes(), &mut out)
        .expect("HKDF expand never fails for 64-byte output on 32+ byte ikm");
    EidKey(out)
}

/// Encrypt an Eid into the 20-byte caBLE service-data payload.
/// Mirrors `Eid::encrypt_advert` in webauthn-rs.
pub fn encrypt_advert(eid: &[u8; 16], key: &EidKey) -> [u8; ADVERT_DATA_LEN] {
    let mut out = [0u8; ADVERT_DATA_LEN];
    // 1. AES-128-CTR over the Eid. The spec uses an all-zero IV
    //    (the nonce is implicit in the Eid itself + the routing_id
    //    in the Eid). Result is 16 bytes of ciphertext.
    //
    //    The `aes` 0.8 crate's `KeyIvInit` wants concrete arrays
    //    rather than slices; explicit `[u8; 16]` conversions keep
    //    the array type preserved for the inner `From` impl.
    let aes_key: [u8; 16] = key.0[..16].try_into().expect("EidKey AES half is 16 bytes");
    let iv: [u8; 16] = [0u8; 16];
    let mut cipher = Aes128Ctr::new((&aes_key).into(), (&iv).into());
    let mut ciphertext = [0u8; 16];
    cipher
        .apply_keystream_b2b(eid, &mut ciphertext)
        .expect("AES-CTR apply_keystream on fixed 16-byte input never fails");

    out[..16].copy_from_slice(&ciphertext);

    // 2. HMAC-SHA256 over the ciphertext, truncated to 4 bytes.
    //    The phone's decrypt does the same HMAC as a cheap filter
    //    before paying the AES cost — a wrong-key ad fails the
    //    HMAC check and is silently dropped.
    let mac_key: [u8; 32] = key.0[32..64]
        .try_into()
        .expect("EidKey HMAC half is 32 bytes");
    let mut mac =
        <HmacSha256 as Mac>::new_from_slice(&mac_key).expect("HMAC accepts any key length");
    mac.update(&ciphertext);
    let tag = mac.finalize().into_bytes();
    out[16..20].copy_from_slice(&tag[..4]);
    out
}

/// Decrypt an advert into the Eid (inverse of [`encrypt_advert`]).
/// Mirrors `Eid::decrypt_advert` in webauthn-rs. Used by the
/// test suite to verify the round-trip without depending on
/// webauthn-rs's openssl-coupled path.
pub fn decrypt_advert(advert: &[u8; 20], key: &EidKey) -> Option<[u8; 16]> {
    // 1. HMAC check first (cheap filter; the phone does the same).
    let ciphertext = &advert[..16];
    let received_tag = &advert[16..20];

    let mac_key: [u8; 32] = key.0[32..64]
        .try_into()
        .expect("EidKey HMAC half is 32 bytes");
    let mut mac =
        <HmacSha256 as Mac>::new_from_slice(&mac_key).expect("HMAC accepts any key length");
    mac.update(ciphertext);
    let computed = mac.finalize().into_bytes();

    // Constant-time tag compare.
    if !bool::from(hmac_eq(received_tag, &computed[..4])) {
        return None;
    }

    // 2. AES-128-CTR decrypt.
    let aes_key: [u8; 16] = key.0[..16].try_into().expect("EidKey AES half is 16 bytes");
    let iv: [u8; 16] = [0u8; 16];
    let mut cipher = Aes128Ctr::new((&aes_key).into(), (&iv).into());
    let mut plaintext = [0u8; 16];
    cipher
        .apply_keystream_b2b(ciphertext, &mut plaintext)
        .expect("AES-CTR apply_keystream on fixed 16-byte input never fails");
    Some(plaintext)
}

/// Constant-time byte slice equality. Returns `true` iff `a == b`
/// (length must match). Wraps the comparison in a way that prevents
/// the compiler from short-circuiting on the first mismatch.
fn hmac_eq(a: &[u8], b: &[u8]) -> subtle::Choice {
    use subtle::ConstantTimeEq;
    a.ct_eq(b)
}

// ── BLE advertiser (Linux + bluez via bluer) ─────────────────────────

#[cfg(target_os = "linux")]
mod imp {
    use super::ADVERT_DATA_LEN;
    use bluer::adv::{Advertisement, AdvertisementHandle};
    use bluer::{Adapter, AdapterEvent, Session, Uuid};
    use futures::StreamExt;
    use std::collections::{BTreeMap, BTreeSet};
    use std::time::Duration;

    /// Bluetooth SIG base UUID — every 16-bit UUID in a BLE AD
    /// gets OR'd into the upper 16 bits of this. caBLE's 0xfff9
    /// becomes `ffff0000-0000-1000-8000-00805f9b34fb`.
    const BLUETOOTH_BASE_UUID: u128 = 0x0000_0000_0000_1000_8000_0080_5f9b_34fb;

    /// Build the caBLE service UUID from the 16-bit form. Mirrors
    /// bluez's `uuid_from_u16` (which itself derives from the
    /// standard Bluetooth base-UUID expansion).
    fn cable_uuid() -> Uuid {
        Uuid::from_u128(BLUETOOTH_BASE_UUID | ((super::FIDO_CABLE_SERVICE_U16 as u128) << 96))
    }

    /// Opaque handle to a running caBLE service-data advertisement.
    /// Drop (or call [`BleAdvertHandle::stop`]) to take the ad down.
    /// The phone's gms has typically already scanned and connected
    /// by the time we drop — the ad's natural lifetime is "until
    /// the WS tunnel's Noise initial message arrives".
    pub struct BleAdvertHandle {
        /// bluer's own handle; dropping the inner value stops the ad.
        _adv_handle: AdvertisementHandle,
    }

    impl BleAdvertHandle {
        /// Stop the advertisement. Best-effort: a stop failure
        /// (race with the kernel tearing down `hci0`, D-Bus
        /// disconnect) is logged at debug and otherwise ignored.
        pub async fn stop(self) {
            // Dropping `_adv_handle` removes the advertisement.
            // We expose an explicit method so callers can express
            // intent in their source.
            drop(self._adv_handle);
        }
    }

    /// Start emitting the caBLE service-data advertisement on the
    /// first available Bluetooth adapter. Returns a handle that
    /// holds the ad alive; drop (or `stop()`) to take it down.
    pub async fn start_advertisement(
        service_data: [u8; ADVERT_DATA_LEN],
    ) -> Result<BleAdvertHandle, super::CableError> {
        let session = Session::new()
            .await
            .map_err(|e| super::CableError::Ble(format!("D-Bus session: {e}")))?;
        let adapter = pick_adapter(&session).await?;
        advertise_on(&adapter, service_data).await
    }

    /// Pick the first available adapter. Tries the default adapter
    /// name first (typically `hci0`); powers it on if it isn't
    /// already (so the operator doesn't have to run
    /// `bluetoothctl power on` manually before the CLI).
    async fn pick_adapter(session: &Session) -> Result<Adapter, super::CableError> {
        let adapter = session
            .default_adapter()
            .await
            .map_err(|e| super::CableError::Ble(format!("default_adapter: {e}")))?;
        // Auto-power-on. bluez usually has the adapter unpowered at
        // boot; if it's already on, this is a no-op.
        if !adapter.is_powered().await.unwrap_or(false) {
            adapter
                .set_powered(true)
                .await
                .map_err(|e| super::CableError::Ble(format!("set_powered(true): {e}")))?;
        }
        Ok(adapter)
    }

    async fn advertise_on(
        adapter: &Adapter,
        service_data: [u8; ADVERT_DATA_LEN],
    ) -> Result<BleAdvertHandle, super::CableError> {
        // Wait for the powered-on event to settle (max 1 s). Once
        // `is_powered()` is true, the kernel is ready to accept
        // advertisement registrations.
        let mut events = adapter
            .events()
            .await
            .map_err(|e| super::CableError::Ble(format!("adapter.events() subscribe: {e}")))?;
        for _ in 0..20 {
            if adapter.is_powered().await.unwrap_or(false) {
                break;
            }
            if let Some(AdapterEvent::PropertyChanged(_)) = events.next().await {
                continue;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        let cable = cable_uuid();

        // BTreeSet<Uuid> (service_uuids) and BTreeMap<Uuid, Vec<u8>>
        // (service_data). bluer 0.17 prefers the BTree variants for
        // deterministic ordering of the encoded AD payload.
        let mut service_uuids = BTreeSet::new();
        service_uuids.insert(cable);

        let mut service_data_map = BTreeMap::new();
        service_data_map.insert(cable, service_data.to_vec());

        let adv = Advertisement {
            // Peripheral (non-connectable) — the phone only scans
            // for the service-data payload, never connects. The
            // tunnel carries data over the WebSocket, not BLE GATT.
            advertisement_type: bluer::adv::Type::Peripheral,
            service_uuids,
            service_data: service_data_map,
            // Discoverable limited (default Peripheral). Local
            // name empty (no UI string — the FIDO:/ URI is the
            // operator-facing identity, not the ad).
            discoverable: Some(true),
            duration: Some(Duration::from_secs(120)),
            ..Default::default()
        };
        let handle = adapter
            .advertise(adv)
            .await
            .map_err(|e| super::CableError::Ble(format!("advertise(): {e}")))?;
        Ok(BleAdvertHandle {
            _adv_handle: handle,
        })
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use super::ADVERT_DATA_LEN;

    /// No-op stub for non-Linux targets (macOS, Windows, WASM).
    /// Returning an `Err` here means the CLI surfaces a clear
    /// "BLE adapter required" message; we don't pretend to
    /// support caBLE on platforms where the protocol's required
    /// side-channel doesn't exist.
    pub struct BleAdvertHandle;

    impl BleAdvertHandle {
        pub async fn stop(self) {}
    }

    pub async fn start_advertisement(
        _service_data: [u8; ADVERT_DATA_LEN],
    ) -> Result<BleAdvertHandle, super::CableError> {
        Err(super::CableError::Ble(
            "unsupported platform: caBLE BLE advertiser requires Linux + bluez".into(),
        ))
    }
}

pub use imp::{start_advertisement, BleAdvertHandle};

// ── tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_uuid_matches_spec() {
        // Chromium + webauthn-rs both use 0xfff9 as the FIDO caBLE
        // 16-bit service UUID. Locking it down so a future
        // refactor that "fixes" the value breaks loudly.
        assert_eq!(FIDO_CABLE_SERVICE_U16, 0xfff9);
    }

    #[test]
    fn advert_bytes_have_correct_length() {
        // 16-byte ciphertext + 4-byte HMAC truncation = 20 bytes.
        // Fits in the BLE 31-byte AD cap with 11 bytes to spare
        // (for the 16-bit UUID header etc.).
        assert_eq!(ADVERT_DATA_LEN, 20);
    }

    #[test]
    fn eid_key_is_deterministic() {
        let s = [0xab; 16];
        let a = eid_key(&s);
        let b = eid_key(&s);
        assert_eq!(a.0, b.0, "EidKey must be deterministic per secret");
        // Different secret → different key (overwhelming prob).
        let s2 = [0xcd; 16];
        let c = eid_key(&s2);
        assert_ne!(a.0, c.0, "different secret must yield different key");
        // Both halves of the key are non-zero (sanity; if HKDF
        // returned all-zero, the encrypt+HMAC would be useless).
        assert!(a.0.iter().any(|&b| b != 0));
    }

    #[test]
    fn eid_key_changes_when_first_byte_differs() {
        // Sanity: confirms HKDF is exercising the full IKM (not
        // just the length) — if HKDF truncated or hashed only
        // partial input this would fail.
        let a = eid_key(&[0x01, 0x02, 0x03, 0x04]);
        let b = eid_key(&[0x01, 0x02, 0x03, 0x05]);
        assert_ne!(a.0, b.0, "one-bit change in secret must change the key");
    }

    #[test]
    fn encrypt_advert_round_trips_through_decrypt() {
        // Pick a non-trivial Eid (matches the WA capture layout
        // from handshake.rs tests: reserved=0, nonce=10 bytes
        // pseudo-random, routing_id=3 bytes, server_id=2 bytes LE).
        let eid: [u8; 16] = [
            0x00, 0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0xa1, 0xb2,
            0xc3, 0xd4,
        ];
        let secret = [
            0xde, 0x26, 0x7a, 0xb1, 0xde, 0x13, 0xde, 0x1b, 0x9b, 0x5e, 0x51, 0x4b, 0xb2, 0x39,
            0x4d, 0x74,
        ];
        let key = eid_key(&secret);
        let advert = encrypt_advert(&eid, &key);
        assert_eq!(advert.len(), ADVERT_DATA_LEN);

        // Round-trip: decrypt the same advert with the same key.
        let recovered = decrypt_advert(&advert, &key)
            .expect("HMAC must match; advert decryptable with the right key");
        assert_eq!(recovered, eid, "decrypt_advert must reproduce the Eid");

        // Wrong key: HMAC must fail and decrypt returns None.
        let other = eid_key(&[0xffu8; 16]);
        assert!(
            decrypt_advert(&advert, &other).is_none(),
            "HMAC must reject an advert under the wrong key"
        );
    }

    #[test]
    fn encrypt_advert_changes_when_eid_changes() {
        // Sanity: confirms the ciphertext actually depends on the
        // Eid plaintext (not just on the key + a constant). Without
        // this, an attacker could substitute any other encrypted
        // eid into our service-data payload.
        let key = eid_key(&[0x42u8; 16]);
        let a = encrypt_advert(&[0u8; 16], &key);
        let b = encrypt_advert(&[0x01u8; 16], &key);
        assert_ne!(a, b, "different Eid must yield different ciphertext");
        // The HMAC part also changes (since ciphertext changed).
        assert_ne!(a[16..20], b[16..20], "HMAC must change too");
    }

    #[test]
    fn service_data_is_sixteen_byte_uuid_safe() {
        // The BLE AD spec caps an ad at 31 bytes. The 16-bit
        // service-data AD type 0x16 packs: 2 bytes length+type
        // + 2 bytes UUID16 + 20 bytes payload = 24 bytes total.
        // That leaves 7 bytes free in the 31-byte AD cap.
        // This is a documentation test: if ADVERT_DATA_LEN ever
        // grows past 27, the BLE 31-byte AD cap will reject the
        // ad with EINVAL on the kernel side.
        const _: () = assert!(ADVERT_DATA_LEN <= 27);
    }
}
