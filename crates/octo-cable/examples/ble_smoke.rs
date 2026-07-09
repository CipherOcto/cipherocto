//! Live BLE advertisement smoke test for caBLE v2.
//!
//! Starts a service-data advertisement on the default Bluetooth
//! adapter (UUID 0xfff9 with a known payload), holds it alive for
//! `HOLD_SECS`, and tears it down. While alive, the operator can
//! verify the ad is on-air via:
//!
//!   $ bluetoothctl scan on
//!   $ busctl introspect org.bluez /org/bluez/hci0 org.bluez.LEAdvertisement1
//!
//! Failure modes surface as `CableError::Ble(...)` printed to
//! stderr; the binary exits non-zero. Success exits 0 after the
//! hold period.
//!
//! Run via: `cargo run -p octo-cable --example ble_smoke`

use std::time::Duration;

use octo_cable::ble::{eid_key, encrypt_advert, start_advertisement, ADVERT_DATA_LEN};
use octo_cable::build_eid;
use rand::RngCore;
use tracing_subscriber::EnvFilter;

const HOLD_SECS: u64 = 30;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Mirror the CLI's redaction layer: a minimal FormatEvent that
    // emits `target level message` per line, with timestamps so the
    // operator can correlate with `busctl introspect` polls.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("debug,octo_cable::ble=trace")),
        )
        .event_format(
            tracing_subscriber::fmt::format::Format::default()
                .with_timer(tracing_subscriber::fmt::time::SystemTime),
        )
        .with_writer(std::io::stderr)
        .init();

    // Construct a known-shape Eid (16 bytes):
    //   byte 0: 0 (reserved)
    //   bytes 1..11: 10-byte random nonce
    //   bytes 11..14: 3-byte routing_id (zeros — relay will assign its own)
    //   bytes 14..16: tunnel_server_id = 0 (LE u16)
    let mut nonce = [0u8; 10];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    let routing_id = [0xAA, 0xBB, 0xCC];
    let eid = build_eid(&nonce, &routing_id, 0);

    // Pick a fixed 16-byte "secret" so the EidKey is deterministic
    // and matches the cablescan APK's TEST_SECRET (round-trip test).
    let secret = [0x42u8; 16];
    let key = eid_key(&secret);

    let advert_bytes = encrypt_advert(&eid, &key);
    assert_eq!(advert_bytes.len(), ADVERT_DATA_LEN);

    eprintln!("[ble_smoke] secret     = {:02x?}", secret);
    eprintln!("[ble_smoke] nonce      = {:02x?}", nonce);
    eprintln!("[ble_smoke] eid        = {:02x?}", eid);
    eprintln!("[ble_smoke] eid_key[0..16]   = {:02x?}", &key.0[0..16]);
    eprintln!("[ble_smoke] eid_key[32..48]  = {:02x?}", &key.0[32..48]);
    eprintln!("[ble_smoke] advert     = {:02x?}", advert_bytes);
    eprintln!("[ble_smoke] starting advertisement on hci0 (UUID 0xfff9)...");
    let _handle = start_advertisement(advert_bytes).await?;
    eprintln!("[ble_smoke] advertisement active; holding for {HOLD_SECS}s");
    eprintln!("[ble_smoke] verify with: bluetoothctl scan on");
    eprintln!("[ble_smoke] or: busctl tree org.bluez | grep LEAdvertisement");

    tokio::time::sleep(Duration::from_secs(HOLD_SECS)).await;

    eprintln!("[ble_smoke] tearing down advertisement");
    Ok(())
}
