//! `whatsapp_xx_session_probe` — Phase 7.J.4: open a real TCP+TLS socket
//! to `e.web.whatsapp.com:5222` (the WA WS endpoint Chrome uses on
//! reconnect, confirmed by `whatsapp_chrome_reconnect_observer`) and
//! send exactly the WA WS frame that Chrome sends as frame[0] of a fresh
//! Noise XX HandshakeInit.
//!
//! We rebuild frame[0] from Chrome's captured bytes (see
//! `docs/research/2026-07-14-chrome-reconnect-handshake.md`):
//!
//!     [57 41]                  = "WA" magic
//!     [06 03 00 00]            = 0x00000306 LE (binary frame version + length)
//!     [24 12 22 0a 20]         = WA binary token + length + protobuf tag
//!                                (field 1, wire type 2, length 32)
//!     [32B e_static_pub]       = ephemeral X25519 public key
//!
//! We use `x25519-dalek` to generate the ephemeral keypair. The identity
//! key from `default.session.db` is NOT used in frame[0] — it goes in
//! frame[2] (out of scope for this probe). The goal here is to see what
//! the server does when it receives a syntactically correct WA WS
//! HandshakeInit opener.
//!
//! Outcomes we expect:
//!
//!  * Server replies with ~350 B starting `00 01 5b 1a d8 02 0a 20` (= Chrome's
//!    frame[1] shape) → wire shape OK, our e_static_pub is structurally valid.
//!  * Server closes connection (no reply or RST) → wire shape rejected at the
//!    version/length field. Means our header bytes are wrong.
//!  * Server replies with a different prefix → WA changed the binary envelope
//!    (we'd need a new chrome capture to update).
//!
//! Run:
//!     cargo run -p octo-adapter-whatsapp --bin whatsapp_xx_session_probe --release
//!
//! Output (stdout):
//!
//!     == whatsapp_xx_session_probe ==
//!     server           : e.web.whatsapp.com:5222
//!     frame[0] sent    : 43 B (hex dump)
//!     server reply     : 350 B (or N/A, or "closed")
//!     server reply head: <first 48 hex bytes>
//!     verdict          : "matches chrome frame[1] shape" / "different shape" /
//!                        "no reply (closed)"
//!
//! Local-only / no push. Standalone investigation binary — does not touch any
//! existing binary or shared code.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use rustls::ClientConfig;
use rustls_pki_types::ServerName;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_rustls::TlsConnector;
use x25519_dalek::{EphemeralSecret, PublicKey};

use octo_adapter_whatsapp::store::StoolapStore;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:?}");
            ExitCode::from(1)
        }
    }
}

async fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let session_path: PathBuf = if args.len() >= 2 {
        PathBuf::from(&args[1])
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(format!(
            "{home}/.local/share/octo/whatsapp/default.session.db"
        ))
    };
    if !session_path.exists() {
        anyhow::bail!("session path does not exist: {}", session_path.display());
    }
    let store = StoolapStore::new(&session_path).context("open session db")?;
    let Some((
        noise_key,
        _identity_key,
        _signed_pre_key,
        push_name,
        avp,
        avs,
        avt,
        registration_id,
    )) = store.read_device_keys().context("read device row")?
    else {
        anyhow::bail!("no device row in {}", session_path.display());
    };
    println!("== whatsapp_xx_session_probe ==");
    println!("session path    : {}", session_path.display());
    println!("registration_id : {registration_id}");
    println!("app_version     : {avp}.{avs}.{avt}");
    println!("push_name       : {push_name:?}");
    println!(
        "noise_key sha   : {}",
        hex::encode(Sha256::digest(&noise_key))
    );

    let server_host = "web.whatsapp.com";
    let server_port = 5222u16;

    // Generate ephemeral X25519 keypair.
    let ephemeral = EphemeralSecret::random_from_rng(rand::rngs::OsRng);
    let e_static_pub = PublicKey::from(&ephemeral);
    let e_static_pub_bytes = e_static_pub.to_bytes();

    // Build frame[0] matching Chrome's observed wire shape exactly.
    let mut frame0 = Vec::with_capacity(43);
    frame0.extend_from_slice(&[0x57, 0x41]); // "WA"
    frame0.extend_from_slice(&[0x06, 0x03, 0x00, 0x00]); // 0x00000306 LE
    frame0.extend_from_slice(&[0x24, 0x12, 0x22, 0x0a, 0x20]); // WA binary token + protobuf tag
    frame0.extend_from_slice(&e_static_pub_bytes);
    assert_eq!(frame0.len(), 43, "frame[0] must be exactly 43B");

    println!();
    println!("server          : {server_host}:{server_port}");
    println!("frame[0] hex    : {}", hex::encode(&frame0));
    println!("frame[0] decoded: WA + 0x06030000 + 0x2412220a20 + 32B e_static_pub");

    // Install ring as the rustls crypto provider. install_default() returns
    // Err(_existing_) if a provider is already installed; we don't care which
    // one is active — we just need *some* provider registered for the static
    // builder below.
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();

    // TLS connect (rustls + webpki-roots + ring).
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(config));
    let server_name = ServerName::try_from(server_host).context("invalid SNI hostname")?;

    let tcp = TcpStream::connect((server_host, server_port))
        .await
        .with_context(|| format!("TCP connect to {server_host}:{server_port}"))?;
    let mut tls = connector
        .connect(server_name, tcp)
        .await
        .context("TLS handshake")?;
    println!("tls handshake   : OK (rustls + ring + webpki-roots)");

    // WebSocket upgrade (RFC 6455). WA's :5222 endpoint is WS-over-TLS, not
    // raw TCP+Noise. Without the upgrade, the server returns HTTP 400 (we
    // observed this on the first run).
    use rand::RngCore;
    let mut key_bytes = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut key_bytes);
    let ws_key = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, key_bytes);
    let upgrade_req = format!(
        "GET /ws/chat HTTP/1.1\r\n\
         Host: {server_host}:{server_port}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: {ws_key}\r\n\
         Sec-WebSocket-Version: 13\r\n\
         \r\n"
    );
    tls.write_all(upgrade_req.as_bytes())
        .await
        .context("write WS upgrade")?;

    // Read full HTTP response (terminated by \r\n\r\n).
    let mut http_buf = Vec::with_capacity(512);
    let mut tmp = [0u8; 256];
    let http_deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let remain = http_deadline.saturating_duration_since(std::time::Instant::now());
        if remain.is_zero() {
            anyhow::bail!("WS upgrade response timeout");
        }
        match timeout(remain, tls.read(&mut tmp)).await {
            Ok(Ok(0)) => anyhow::bail!("server closed during WS upgrade"),
            Ok(Ok(n)) => {
                http_buf.extend_from_slice(&tmp[..n]);
                if http_buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            Ok(Err(e)) => anyhow::bail!("WS upgrade read err: {e}"),
            Err(_) => anyhow::bail!("WS upgrade timeout"),
        }
    }
    let http_str = std::str::from_utf8(&http_buf).context("upgrade response not UTF-8")?;
    let status_line = http_str.lines().next().unwrap_or("");
    println!("ws upgrade resp : {status_line}");
    if !status_line.contains(" 101 ") {
        println!(
            "verdict         : WS UPGRADE REJECTED (server did not return 101, body={:?})",
            &http_str[..http_str.len().min(200)]
        );
        let _ = tls.shutdown().await;
        return Ok(());
    }
    println!("ws upgrade      : OK (101 Switching Protocols — tunnel established)");

    // Wrap Chrome's frame[0] in a masked binary WS frame (RFC 6455 §5.3).
    // Use 7-bit len for the 43B XX opener; same helper also handles 16-/64-bit
    // extended lengths so IK probes with >125B payloads work too.
    let mut ws_frame = Vec::with_capacity(10 + 4 + frame0.len());
    ws_frame.push(0x82); // FIN + binary (opcode 2)
    let payload_len = frame0.len();
    if payload_len < 126 {
        ws_frame.push(0x80 | payload_len as u8); // MASK + 7-bit len
    } else if payload_len < 65536 {
        ws_frame.push(0x80 | 126);
        ws_frame.extend_from_slice(&(payload_len as u16).to_be_bytes());
    } else {
        ws_frame.push(0x80 | 127);
        ws_frame.extend_from_slice(&(payload_len as u64).to_be_bytes());
    }
    let mut mask = [0u8; 4];
    rand::rngs::OsRng.fill_bytes(&mut mask);
    ws_frame.extend_from_slice(&mask);
    for (i, b) in frame0.iter().enumerate() {
        ws_frame.push(b ^ mask[i % 4]);
    }
    tls.write_all(&ws_frame)
        .await
        .context("write WS frame[0]")?;
    println!(
        "frame[0] sent   : 43B (full XX HandshakeInit opener, wrapped in masked WS binary frame)"
    );

    // Read server's WS reply. Server frames are UNMASKED.
    let mut reply: Vec<u8>;
    let mut buf = [0u8; 4096];
    let read_result = timeout(Duration::from_secs(5), tls.read(&mut buf)).await;
    match read_result {
        Ok(Ok(0)) => {
            println!("server reply    : 0 B (EOF — server closed without sending)");
            println!("verdict         : NO REPLY (server closed after WS upgrade)");
        }
        Ok(Ok(n)) => {
            // Strip WS header from the server's frame.
            let payload = if n >= 2 {
                let masked = (buf[1] & 0x80) != 0;
                let mut payload_len = (buf[1] & 0x7f) as usize;
                let mut header_len = 2;
                if payload_len == 126 && n >= 4 {
                    payload_len = u16::from_be_bytes([buf[2], buf[3]]) as usize;
                    header_len = 4;
                } else if payload_len == 127 && n >= 10 {
                    payload_len = u64::from_be_bytes([
                        buf[2], buf[3], buf[4], buf[5], buf[6], buf[7], buf[8], buf[9],
                    ]) as usize;
                    header_len = 10;
                }
                if masked {
                    if n >= header_len + 4 {
                        let mask_key = &buf[header_len..header_len + 4];
                        header_len += 4;
                        let take = (n - header_len).min(payload_len);
                        let mut p = Vec::with_capacity(take);
                        for i in 0..take {
                            p.push(buf[header_len + i] ^ mask_key[i % 4]);
                        }
                        p
                    } else {
                        Vec::new()
                    }
                } else {
                    let take = (n - header_len).min(payload_len);
                    buf[header_len..header_len + take].to_vec()
                }
            } else {
                Vec::new()
            };
            reply = payload;
            println!(
                "server reply    : {n} B (first read, ws-opcode=0x{:x}, payload={}B)",
                n & 0x0f,
                reply.len()
            );
            // Keep reading until EOF or short timeout, in case reply spans multiple reads.
            for _ in 0..4 {
                match timeout(Duration::from_millis(500), tls.read(&mut buf)).await {
                    Ok(Ok(0)) => break,
                    Ok(Ok(m)) => {
                        if m >= 2 {
                            let pl = (buf[1] & 0x7f) as usize;
                            let hl = if pl == 126 {
                                4
                            } else if pl == 127 {
                                10
                            } else {
                                2
                            };
                            let take = (m - hl).min(pl);
                            reply.extend_from_slice(&buf[hl..hl + take]);
                        }
                    }
                    _ => break,
                }
            }
            println!(
                "server reply total: {} B (after WS header strip)",
                reply.len()
            );
            let head_hex = hex::encode(&reply[..reply.len().min(48)]);
            println!("server reply head: {head_hex}");
            // Chrome's frame[1] starts with `00 01 5b 1a d8 02 0a 20`.
            let chrome_prefix = &reply[..reply.len().min(8)];
            let expected_prefix = hex::decode("00015b1ad8020a20").unwrap();
            if reply.len() >= 8 && chrome_prefix == expected_prefix.as_slice() {
                println!(
                    "verdict         : MATCHES CHROME FRAME[1] SHAPE (server accepted opener)"
                );
            } else if reply.len() >= 4 && &chrome_prefix[..4] == b"\x00\x01\x5b\x1a" {
                println!(
                    "verdict         : PARTIAL MATCH (first 4B match Chrome's frame[1] prefix)"
                );
            } else if reply.len() >= 2 && &chrome_prefix[..2] == b"\x57\x41" {
                println!(
                    "verdict         : WA BINARY ENVELOPE (server replies with its own WA frame)"
                );
            } else if reply.is_empty() {
                println!("verdict         : NO REPLY (server closed)");
            } else {
                println!(
                    "verdict         : DIFFERENT SHAPE (server reply starts with {:02x?}, not Chrome's frame[1])",
                    &chrome_prefix[..chrome_prefix.len().min(8)]
                );
            }
        }
        Ok(Err(e)) => {
            println!("server reply    : ERROR ({e})");
            println!("verdict         : READ ERROR");
        }
        Err(_) => {
            println!("server reply    : TIMEOUT (5s, no reply yet — server silent)");
            println!("verdict         : SILENT (possible: server delays first reply until after frame[2])");
        }
    }

    let _ = tls.shutdown().await;
    Ok(())
}
