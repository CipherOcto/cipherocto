//! `whatsapp_ik_session_probe` — Phase 7.J S7 verification binary.
//!
//! Drives wacore's `IkHandshakeState::build_client_hello` against the live
//! WA server using the IK identity persisted in `default.session.db`.
//!
//! Why a separate binary from `whatsapp_xx_session_probe`:
//!     - That binary sends a raw XX opener (ephemeral only, no identity) and
//!       confirms the WA binary envelope is accepted at the WS layer.
//!     - This binary drives the FULL IK path — server_cert_chain reuse, identity
//!       static pub encrypted into frame[0], 0-RTT payload — using the modern
//!       `HandshakeMessage.ClientHello` shape patched in S6.7
//!       (useExtended + extendedCiphertext + pqMode + extendedEphemeral).
//!
//! End-state for S7:
//!
//! - Server replies with frame[1] (server hello, ephemeral + enc(server_static) + enc(cert)). IK ClientHello shape ACCEPTED. The S6.7 patch was the right fix at the handshake layer. Downstream post-handshake IQ 401 may still fire, but the noise layer is no longer the gate. Server closes the connection shortly after with code 1011.
//! - Server replies with 401 / closes connection. Server rejects the modern ClientHello shape. Iterate per S6.5: try XXKEM_2 / IKKEM / IKKEM_FS pqMode variants, replace random placeholders with ECDH-derived extendedCiphertext / extendedEphemeral.
//! - Server replies with `Wa-6` / 460. Noise-layer rejection (cert chain stale, IK not enabled). Means we need to re-pair first.
//!
//! Run:
//!       cargo run -p octo-adapter-whatsapp --bin whatsapp_ik_session_probe --release
//!
//! Output (stdout):
//!       == whatsapp_ik_session_probe ==
//!       session path    : /.../default.session.db
//!       ik_identity_fp  : <16 hex chars>          ← SHA-256 of noise_key (truncated)
//!       server_cert     : Some (N B; not_before=X, not_after=Y)
//!       ik enabled      : true/false              ← leaf validity window vs now
//!       server          : web.whatsapp.com:5222
//!       frame[0] hex    : <N bytes>
//!       ws upgrade resp : HTTP/1.1 101 Switching Protocols
//!       server reply    : <N bytes>
//!       server reply head: <hex>
//!       verdict         : MATCHES WA FRAME[1] SHAPE / DIFFERENT / NO REPLY / 401
//!
//! Local-only / no push. Standalone investigation binary.

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
use x25519_dalek::{PublicKey, StaticSecret};

use octo_adapter_whatsapp::store::StoolapStore;

use whatsapp_rust::wacore::handshake::IkHandshakeState;
use whatsapp_rust::wacore::libsignal::core::curve::KeyPair;

const SERVER_HOST: &str = "web.whatsapp.com";
const SERVER_PORT: u16 = 5222;
// WA connection prologue (4 bytes: 'W', 'A', WA_MAGIC_VALUE=6, DICT_VERSION=3).
// See wacore-binary::consts::WA_CONN_HEADER in the fork (binary/src/consts.rs:22).
const WA_CONN_HEADER: [u8; 4] = [b'W', b'A', 0x06, 0x03];

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
        noise_key_blob,
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

    // Read server_cert_chain JSON via a fresh stoolap connection (parallel
    // to `whatsapp_connect_trace::probe_cert_chain`).
    let cert = read_cert_chain(&session_path).context("read cert chain")?;
    let server_static_pub: [u8; 32] = match &cert {
        Some(c) => c.static_pub,
        None => anyhow::bail!("no server_cert_chain on disk — IK path requires a cached server static pub; pair first"),
    };
    let cert_summary = match &cert {
        Some(c) => format!(
            "Some ({}B; not_before={}, not_after={})",
            c.len, c.not_before, c.not_after
        ),
        None => "None".into(),
    };

    let now_secs: i64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let ik_enabled = cert
        .as_ref()
        .map(|c| now_secs >= c.not_before && now_secs < c.not_after)
        .unwrap_or(false);

    // Parse noise_key blob: 32B priv + 32B pub (raw X25519 key bytes — store
    // uses `public_key_bytes()`, NOT the 33B `serialize()` form). Confirmed by
    // store.rs:1545 (the write path uses `public_key_bytes()`).
    // To feed `KeyPair::from_public_and_private` (which expects the 33B
    // serialized form: 1-byte KeyType prefix + 32B key), we prepend `0x05`
    // (KeyType::Djb per curve.rs:31).
    if noise_key_blob.len() != 64 {
        anyhow::bail!(
            "noise_key blob is {}B, expected 64B (32 priv + 32 pub)",
            noise_key_blob.len()
        );
    }
    let priv_bytes: [u8; 32] = noise_key_blob[0..32].try_into().unwrap();
    let mut pub_bytes = [0u8; 33];
    pub_bytes[0] = 0x05; // KeyType::Djb
    pub_bytes[1..33].copy_from_slice(&noise_key_blob[32..64]);
    let static_kp = KeyPair::from_public_and_private(&pub_bytes, &priv_bytes)
        .context("KeyPair::from_public_and_private")?;

    let ik_identity_fp = hex::encode(&Sha256::digest(&noise_key_blob)[..8]);

    println!("== whatsapp_ik_session_probe ==");
    println!("session path    : {}", session_path.display());
    println!("registration_id : {registration_id}");
    println!("app_version     : {avp}.{avs}.{avt}");
    println!("push_name       : {push_name:?}");
    println!("ik_identity_fp  : {ik_identity_fp} (SHA-256 of noise_key, truncated)");
    println!("server_cert     : {cert_summary}");
    println!("ik enabled      : {ik_enabled} (leaf validity window vs now={now_secs})");
    println!("server          : {SERVER_HOST}:{SERVER_PORT}");
    println!();

    // 0-RTT client payload: for S7 verification we don't need the real AppVersion
    // + DeviceProps payload — we just need something that the Noise cipher can
    // encrypt. The server's verdict on ClientHello shape is determined by the
    // outer envelope + ClientHello fields, not the inner payload. Use 145B of
    // zeros (matches Chrome's observed payload size in
    // `whatsapp_drive_xx_complete.rs`).
    let client_payload = vec![0u8; 145];

    let mut ik_state = IkHandshakeState::new(
        static_kp,
        server_static_pub,
        client_payload,
        &WA_CONN_HEADER,
    )
    .context("IkHandshakeState::new")?;
    let inner_client_hello = ik_state
        .build_client_hello()
        .context("IkHandshakeState::build_client_hello")?;

    // Wrap the IK ClientHello in the WA binary envelope (8B header) and send
    // as the WS frame[0]. Chrome's observed envelope:
    //     57 41            = "WA"
    //     06 03 00 00      = 0x00000306 LE
    //     (then protobuf-encoded HandshakeMessage.client_hello payload)
    // Note: the protobuf tag `0a 20` (field 1 wire-type 2, length 32) is part
    // of the protobuf framing — wacore's `encode_to_vec` produces it.
    let mut frame0 = Vec::with_capacity(8 + inner_client_hello.len());
    frame0.extend_from_slice(&WA_CONN_HEADER);
    frame0.extend_from_slice(&inner_client_hello);
    println!(
        "frame[0] inner  : {}B (IK ClientHello)",
        inner_client_hello.len()
    );
    println!(
        "frame[0] total  : {}B (WA envelope + protobuf)",
        frame0.len()
    );
    println!("frame[0] hex    : {}", hex::encode(&frame0));
    println!();

    // Install ring as the rustls crypto provider (idempotent).
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();

    // TLS connect.
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(config));
    let server_name = ServerName::try_from(SERVER_HOST).context("invalid SNI hostname")?;

    let tcp = TcpStream::connect((SERVER_HOST, SERVER_PORT))
        .await
        .with_context(|| format!("TCP connect to {SERVER_HOST}:{SERVER_PORT}"))?;
    let mut tls = connector
        .connect(server_name, tcp)
        .await
        .context("TLS handshake")?;
    println!("tls handshake   : OK (rustls + ring + webpki-roots)");

    // WebSocket upgrade (RFC 6455).
    use rand::RngCore;
    let mut key_bytes = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut key_bytes);
    let ws_key = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, key_bytes);
    let upgrade_req = format!(
        "GET /ws/chat HTTP/1.1\r\n\
         Host: {SERVER_HOST}:{SERVER_PORT}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: {ws_key}\r\n\
         Sec-WebSocket-Version: 13\r\n\
         \r\n"
    );
    tls.write_all(upgrade_req.as_bytes())
        .await
        .context("write WS upgrade")?;

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

    // Send IK ClientHello as masked WS binary frame. Use RFC 6455 §5.3
    // length encoding: 7-bit for ≤125, 16-bit extended for 126–65535,
    // 64-bit extended for >65535. Critical: cast `len() as u8` would
    // truncate and produce a malformed frame on the wire (server replied
    // 1002 protocol error on a 375B frame before this fix).
    let mut ws_frame = Vec::with_capacity(10 + 4 + frame0.len());
    ws_frame.push(0x82); // FIN + binary (opcode 2)
    let payload_len = frame0.len();
    if payload_len < 126 {
        ws_frame.push(0x80 | payload_len as u8); // MASK + 7-bit len
    } else if payload_len < 65536 {
        ws_frame.push(0x80 | 126); // MASK + 16-bit extended
        ws_frame.extend_from_slice(&(payload_len as u16).to_be_bytes());
    } else {
        ws_frame.push(0x80 | 127); // MASK + 64-bit extended
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
        "frame[0] sent   : {}B (IK ClientHello + WA envelope, masked WS binary)",
        frame0.len()
    );

    // Read server's WS reply.
    let mut reply: Vec<u8>;
    let mut buf = [0u8; 4096];
    let read_result = timeout(Duration::from_secs(15), tls.read(&mut buf)).await;
    match read_result {
        Ok(Ok(0)) => {
            println!("server reply    : 0 B (EOF — server closed without sending)");
            println!("verdict         : NO REPLY (server closed after WS upgrade)");
        }
        Ok(Ok(n)) => {
            let payload = strip_ws_header(&buf[..n]);
            reply = payload;
            println!(
                "server reply    : {n} B (first read, payload={}B)",
                reply.len()
            );
            for _ in 0..4 {
                match timeout(Duration::from_millis(500), tls.read(&mut buf)).await {
                    Ok(Ok(0)) => break,
                    Ok(Ok(m)) => {
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
                    _ => break,
                }
            }
            println!(
                "server reply total: {} B (after WS header strip)",
                reply.len()
            );
            let head_hex = hex::encode(&reply[..reply.len().min(48)]);
            println!("server reply head: {head_hex}");

            // IK ServerHello shape (after WA envelope):
            //     00 01 <length> <protobuf>
            //   = field 1 wire-type 2 (ServerHello), length-prefixed.
            //   First byte of protobuf is `0a 20` (field 1, ephemeral, len 32)
            //   so first 8 bytes of payload are `00 01 XX XX 0a 20 YY YY`.
            // We don't strict-match here — just print what we got.
            if reply.is_empty() {
                println!("verdict         : NO REPLY (server closed)");
            } else if reply.len() >= 8 && reply[..4] == [0x00, 0x01, 0x5b, 0x1a] {
                println!(
                    "verdict         : MATCHES WA SERVER HELLO SHAPE (server accepted IK ClientHello, returned ServerHello)"
                );
            } else if reply.len() >= 2 && &&reply[..2] == b"\x57\x41" {
                println!(
                    "verdict         : WA BINARY ENVELOPE (server replied with its own WA frame)"
                );
            } else if reply.len() >= 4 && &&reply[..4] == b"HTTP" {
                println!("verdict         : HTTP REPLY (unexpected — got raw HTTP body)");
            } else {
                println!(
                    "verdict         : DIFFERENT SHAPE (server reply starts with {:02x?})",
                    &reply[..reply.len().min(16)]
                );
            }
        }
        Ok(Err(e)) => {
            println!("server reply    : ERROR ({e})");
            println!("verdict         : READ ERROR");
        }
        Err(_) => {
            println!("server reply    : TIMEOUT (5s, no reply yet)");
            println!("verdict         : SILENT");
        }
    }

    let _ = tls.shutdown().await;
    Ok(())
}

struct CertInfo {
    len: usize,
    not_before: i64,
    not_after: i64,
    static_pub: [u8; 32],
}

fn read_cert_chain(session_path: &std::path::Path) -> Result<Option<CertInfo>> {
    use anyhow::Context;
    let dsn = format!("file://{}", session_path.display());
    let db = octo_storage_core::Database::open(&dsn).context("stoolap open")?;
    let mut rows = db
        .query("SELECT server_cert_chain FROM device WHERE id = 1", ())
        .context("SELECT server_cert_chain")?;
    let row = match rows.next() {
        Some(Ok(r)) => r,
        _ => return Ok(None),
    };
    let chain_bytes: Vec<u8> = row
        .get(0)
        .context("get cert chain bytes")
        .unwrap_or_default();
    if chain_bytes.is_empty() {
        return Ok(None);
    }
    let v: serde_json::Value = serde_json::from_slice(&chain_bytes)
        .context("server_cert_chain JSON decode (CACHED_SCHEMA?)")?;
    let leaf = v.get("leaf").context("leaf field")?;
    let nb = leaf
        .get("not_before")
        .and_then(|x| x.as_i64())
        .context("leaf.not_before")?;
    let na = leaf
        .get("not_after")
        .and_then(|x| x.as_i64())
        .context("leaf.not_after")?;
    // The JSON `key` field is a JSON array of integers — wacore serializes
    // `[u8; 32]` as `[u8; 32]` (via serde_json's default for byte arrays),
    // not as hex. Confirmed live via `whatsapp_session_introspect` against
    // default.session.db: `intermediate.key` is 32 ints, `leaf.key` is 32 ints.
    let key_arr = leaf
        .get("key")
        .and_then(|x| x.as_array())
        .context("leaf.key must be a JSON array of integers")?;
    anyhow::ensure!(
        key_arr.len() == 32,
        "leaf.key has {} ints, expected 32",
        key_arr.len()
    );
    let mut static_pub = [0u8; 32];
    for (i, v) in key_arr.iter().enumerate() {
        let n = v
            .as_u64()
            .with_context(|| format!("leaf.key[{i}] is not an integer"))?;
        anyhow::ensure!(n <= 255, "leaf.key[{i}] = {n} exceeds u8");
        static_pub[i] = n as u8;
    }
    Ok(Some(CertInfo {
        len: chain_bytes.len(),
        not_before: nb,
        not_after: na,
        static_pub,
    }))
}

fn strip_ws_header(buf: &[u8]) -> Vec<u8> {
    if buf.len() < 2 {
        return Vec::new();
    }
    let masked = (buf[1] & 0x80) != 0;
    let mut payload_len = (buf[1] & 0x7f) as usize;
    let mut header_len = 2;
    if payload_len == 126 && buf.len() >= 4 {
        payload_len = u16::from_be_bytes([buf[2], buf[3]]) as usize;
        header_len = 4;
    } else if payload_len == 127 && buf.len() >= 10 {
        payload_len = u64::from_be_bytes([
            buf[2], buf[3], buf[4], buf[5], buf[6], buf[7], buf[8], buf[9],
        ]) as usize;
        header_len = 10;
    }
    if masked {
        if buf.len() >= header_len + 4 {
            let mask_key = &buf[header_len..header_len + 4];
            header_len += 4;
            let take = (buf.len() - header_len).min(payload_len);
            let mut p = Vec::with_capacity(take);
            for i in 0..take {
                p.push(buf[header_len + i] ^ mask_key[i % 4]);
            }
            p
        } else {
            Vec::new()
        }
    } else {
        let take = (buf.len() - header_len).min(payload_len);
        buf[header_len..header_len + take].to_vec()
    }
}

// x25519-dalek symbols retained for parity with the xx probe in case we
// need to fallback to a manual wire shape later.
#[allow(dead_code)]
fn _unused_x25519(_priv_bytes: &[u8; 32]) -> [u8; 32] {
    let s = StaticSecret::from(*_priv_bytes);
    let p = PublicKey::from(&s);
    *p.as_bytes()
}
