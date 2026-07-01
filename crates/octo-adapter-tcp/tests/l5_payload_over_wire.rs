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
/// reach the listener contain the envelope and payload verbatim.
///
/// Wire format (RFC-0850 §8.8, Raw mode, single-frame):
///   `[4-byte total_len][envelope wire bytes][payload bytes]`
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

    // New wire format: single length-prefixed frame
    //   [4-byte total_len][envelope bytes][payload bytes]
    let env_len = envelope_bytes.len();
    let payload_len = payload.len();
    let total_len = env_len + payload_len;

    // Search for the total_len prefix to locate the frame
    let prefix = (total_len as u32).to_be_bytes();
    let mut found_at = None;
    for i in 0..bytes.len().saturating_sub(4) {
        if bytes[i..i + 4] == prefix {
            found_at = Some(i);
            break;
        }
    }
    let frame_start =
        found_at.expect("total-length prefix must appear in captured wire bytes");

    // 8. Verify wire envelope bytes (right after the total_len prefix)
    let env_start = frame_start + 4;
    let env_end = env_start + env_len;
    assert!(
        env_end + payload_len <= bytes.len(),
        "captured bytes too short: need {} envelope + {} payload bytes",
        env_len,
        payload_len
    );
    let wire_envelope = &bytes[env_start..env_end];
    assert_eq!(
        wire_envelope,
        &envelope_bytes[..],
        "wire envelope bytes must match envelope.to_wire_bytes()"
    );

    // 9. Verify wire payload bytes (right after the envelope)
    let pl_start = env_end;
    let pl_end = pl_start + payload_len;
    let wire_payload = &bytes[pl_start..pl_end];
    assert_eq!(
        wire_payload, payload,
        "wire payload bytes must match the payload argument to send_message"
    );
}

/// L5: tcp_adapter_receives_payload_from_wire
///
/// Validates the inbound path: sends a single-frame message via one
/// `TcpAdapter`'s `send_message`, then drains it through a second
/// `TcpAdapter`'s `receive_messages` and confirms `RawPlatformMessage.payload`
/// equals the original envelope-bytes || payload-bytes concatenation.
#[tokio::test(flavor = "multi_thread")]
async fn tcp_adapter_receives_payload_from_wire() {
    use octo_network::dot::adapters::PlatformAdapter;
    use std::collections::BTreeMap;

    // Sender binds an ephemeral port; receiver connects to it.
    let receiver = TcpAdapter::new("127.0.0.1:0".parse::<SocketAddr>().unwrap())
        .await
        .unwrap();
    let recv_addr = receiver.local_addr();
    // Give the accept loop a moment to start
    tokio::time::sleep(Duration::from_millis(50)).await;

    let sender = TcpAdapter::new("127.0.0.1:0".parse::<SocketAddr>().unwrap())
        .await
        .unwrap();
    sender.connect(recv_addr).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    let envelope = DeterministicEnvelope::default();
    let payload: &[u8] = b"inbound L5 payload bytes";
    let domain = BroadcastDomainId::new(PlatformType::Tcp, "recv.example");

    sender
        .send_message(&domain, &envelope, payload)
        .await
        .expect("send_message");

    // Drain the receiver
    let _ = recv_addr; // silence unused
    let domain_drain = BroadcastDomainId::new(PlatformType::Tcp, "recv.example");
    let messages = tokio::time::timeout(
        Duration::from_millis(2000),
        receiver.receive_messages(&domain_drain),
    )
    .await
    .expect("receive_messages timed out")
    .expect("receive_messages error");

    assert!(
        !messages.is_empty(),
        "receiver should have received the inbound frame"
    );
    let raw = &messages[0];
    let expected_frame: Vec<u8> = envelope
        .to_wire_bytes()
        .into_iter()
        .chain(payload.iter().copied())
        .collect();
    assert_eq!(
        raw.payload, expected_frame,
        "RawPlatformMessage.payload should be envelope-bytes || payload-bytes"
    );

    // canonicalize parses the first ENVELOPE_WIRE_LEN bytes
    let canonical = receiver
        .canonicalize(raw)
        .expect("canonicalize should succeed");
    assert_eq!(canonical.envelope_id, envelope.envelope_id);

    // Silence unused
    let _: BTreeMap<String, String> = BTreeMap::new();
}
