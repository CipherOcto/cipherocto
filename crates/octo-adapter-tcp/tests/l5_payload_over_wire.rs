//! L5: TcpAdapter payload wire-format regression tests
//!
//! Verifies TcpAdapter honours RFC-0850 v1.3.0's `send_message(domain, envelope, payload)`
//! signature: the wire frame includes the payload bytes alongside the envelope.
//!
//! Plan reference: `docs/plans/2026-06-28-payload-transport-regression-tests.md` (L5)

use std::net::SocketAddr;
use std::time::Duration;

use octo_adapter_tcp::TcpAdapter;
use octo_network::dot::adapters::PlatformAdapter;
use octo_network::dot::domain::PlatformType;
use octo_network::dot::envelope::DeterministicEnvelope;
use octo_network::dot::BroadcastDomainId;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;

/// Spawn a captor that accepts connections on `listener` for up to `max_conns`
/// connections (each with a short per-connection read timeout) and concatenates
/// the bytes received across all of them. Used to capture the wire frame that
/// `TcpAdapter::send_message` writes through a fresh `TcpStream`.
async fn capture_wire_bytes(
    listener: TcpListener,
    max_conns: usize,
    per_conn_timeout: Duration,
) -> Vec<u8> {
    let mut all_bytes = Vec::new();
    for _ in 0..max_conns {
        let accept = tokio::time::timeout(per_conn_timeout, listener.accept()).await;
        let (mut stream, _) = match accept {
            Ok(Ok(pair)) => pair,
            _ => break,
        };

        let mut buf = Vec::new();
        let _ = tokio::time::timeout(per_conn_timeout, stream.read_to_end(&mut buf)).await;
        all_bytes.extend_from_slice(&buf);
    }
    all_bytes
}

/// L5: tcp_adapter_sends_payload_over_wire
///
/// Sets up a raw `TcpListener`, points a `TcpAdapter` at it, calls
/// `send_message(domain, envelope, payload)`, and verifies the bytes that
/// reach the listener contain the payload (and envelope) verbatim.
///
/// Wire format (RFC-0850 v1.3.0 §8.8):
///   `[4-byte env_len][envelope wire bytes][4-byte payload_len][payload bytes]`
#[tokio::test(flavor = "multi_thread")]
async fn tcp_adapter_sends_payload_over_wire() {
    // 1. Bind a raw listener
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_addr = listener.local_addr().unwrap();

    // 2. Spawn a captor that accepts up to 4 connections on the listener.
    //    adapter.connect() opens connection #1 (captor accepts it; adapter
    //    side keeps it open via reader_loop — read_to_end times out).
    //    send_message() opens connection #2, writes the frame, drops the
    //    stream → captor's second accept reads the frame to EOF.
    let captor = tokio::spawn(capture_wire_bytes(
        listener,
        4,                           // up to 4 accepts
        Duration::from_millis(2000), // per-accept / per-read timeout
    ));

    // 3. Create the TcpAdapter
    let adapter = TcpAdapter::new("127.0.0.1:0".parse::<SocketAddr>().unwrap())
        .await
        .unwrap();

    // 4. Have the adapter register `target_addr` as a peer (also opens conn #1)
    adapter.connect(target_addr).await.unwrap();

    // Give the OS scheduler a moment to land the registration
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 5. Build a deterministic envelope
    let envelope = DeterministicEnvelope::default();
    let envelope_bytes = envelope.to_wire_bytes();
    let payload: &[u8] = b"this is the L5 payload bytes: hello over TCP wire";

    // 6. Send via the adapter (opens conn #2, writes frame, drops stream)
    let domain = BroadcastDomainId::new(PlatformType::Tcp, "test.example.com");
    let receipt = adapter
        .send_message(&domain, &envelope, payload)
        .await
        .expect("send_message should succeed");

    assert!(
        !receipt.platform_message_id.is_empty(),
        "delivery receipt must include a platform message id"
    );

    // 7. Capture wire bytes across all accepts
    let bytes = captor.await.unwrap();

    // Find the captured frame: skip leading 0 bytes (from connection #1 with
    // no data) and locate the envelope length prefix
    let env_len = envelope_bytes.len();
    let payload_len = payload.len();

    // Search the captured bytes for our specific envelope length prefix
    let prefix = (env_len as u32).to_be_bytes();
    let mut found_at = None;
    for i in 0..bytes.len().saturating_sub(4) {
        if bytes[i..i + 4] == prefix {
            found_at = Some(i);
            break;
        }
    }
    let frame_start = found_at.expect("envelope length prefix must appear in captured wire bytes");

    // 8. Verify wire envelope bytes
    let env_end = frame_start + 4 + env_len;
    assert!(
        env_end <= bytes.len(),
        "captured bytes are too short to contain envelope (frame_start={frame_start}, env_len={env_len}, captured_len={})",
        bytes.len()
    );
    let wire_envelope = &bytes[frame_start + 4..env_end];
    assert_eq!(
        wire_envelope,
        &envelope_bytes[..],
        "wire envelope bytes must match envelope.to_wire_bytes()"
    );

    // 9. Verify wire payload bytes
    let pl_off = env_end;
    let pl_len_off = pl_off + 4;
    assert!(
        pl_len_off <= bytes.len(),
        "captured bytes must contain payload length prefix"
    );
    let captured_pl_len =
        u32::from_be_bytes(bytes[pl_off..pl_len_off].try_into().unwrap()) as usize;
    assert_eq!(
        captured_pl_len, payload_len,
        "payload length prefix must equal payload.len()"
    );

    assert!(
        pl_len_off + payload_len <= bytes.len(),
        "captured bytes must contain full payload (need {}, got {})",
        payload_len,
        bytes.len() - pl_len_off
    );
    let wire_payload = &bytes[pl_len_off..pl_len_off + payload_len];
    assert_eq!(
        wire_payload, payload,
        "wire payload bytes must match the payload argument to send_message"
    );
}

/// L5: tcp_adapter_receives_payload_from_wire (DEFERRED)
///
/// This test is currently `#[ignore]`d: it documents the intended behaviour
/// per RFC-0850 v1.3.0 (`receive_messages` returns the payload in
/// `RawPlatformMessage.payload`), but the on-wire reader still expects the
/// envelope-only frame format from RFC-0850 v1.2.0.
///
/// The reader upgrade is tracked as a follow-up ("make the payload readable")
/// once the team decides on the wire-format migration policy (compatibility
/// flag vs clean break).
#[tokio::test(flavor = "multi_thread")]
#[ignore = "TCP reader still expects RFC-0850 v1.2.0 envelope-only frame; reader upgrade tracked as follow-up"]
async fn tcp_adapter_receives_payload_from_wire() {
    // Once the reader is updated, this test will:
    //   1. Spin up two TcpAdapters
    //   2. Push a payload-shaped frame through the wire
    //   3. Assert `receive_messages` returns a RawPlatformMessage whose
    //      `payload` equals the bytes originally passed to `send_message`.
    unimplemented!("reader-side payload parsing — see pending work plan");
}
