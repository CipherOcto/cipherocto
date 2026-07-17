//! `whatsapp_drive_xx_complete` — Phase 7.J.6 (real S5)
//!
//! Drive wacore's XX handshake to COMPLETION against web.whatsapp.com:5222.
//! Logs every frame sent + received (with timing) to disk for comparison
//! against Chrome's captures from `reconnect.jsonl`.
//!
//! Why: previous binary `whatsapp_xx_session_probe` only sends frame[0] and
//! reads frame[1]. We need full XX (frame[0..4]) to know what wacore emits
//! at the wire level — then diff against Chrome's emission.
//!
//! Frame numbering (per Noise XX):
//!   frame[0]   client e (XX opener, 43B)
//!   frame[1]   server hello (e + ee + es + enc(s) + enc(cert))
//!   frame[2]   client finish (enc(s) + enc(payload))
//!   frame[3]   server finish (post-handshake init or ack)
//!
//! Output NDJSON to /tmp/wacore-xx-frames.ndjson with:
//!   {ts_ms, dir: "send"|"recv", idx, len, head_hex}
//!
//! Run:
//!     cargo run -p octo-adapter-whatsapp --bin whatsapp_drive_xx_complete --release

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use base64::Engine;
use rustls::ClientConfig;
use rustls_pki_types::ServerName;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{info, warn};
use x25519_dalek::{PublicKey, StaticSecret};

use whatsapp_rust::wacore::noise::{generate_iv, HandshakeUtils, NoiseHandshake};
use whatsapp_rust::wacore_binary::consts::{NOISE_PATTERN_XX, WA_CONN_HEADER};

const SERVER_HOST: &str = "web.whatsapp.com";
const SERVER_PORT: u16 = 5222;

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("FATAL: {e:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<()> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();

    println!("== whatsapp_drive_xx_complete ==");
    println!("mode             : drive wacore XX to completion + log every frame");
    println!();

    let out_path: PathBuf = "/tmp/wacore-xx-frames.ndjson".into();
    let _ = std::fs::remove_file(&out_path);
    let mut writer = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&out_path)
        .await?;
    let start_ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis();

    let log = |dir: &str, idx: u32, len: usize, head: &[u8]| {
        let head_hex: String = head.iter().take(48).map(|b| format!("{:02x}", b)).collect();
        let ts_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
            - start_ts;
        let line = format!(
            "{{\"ts_ms\":{},\"dir\":\"{}\",\"idx\":{},\"len\":{},\"head_hex\":\"{}\"}}\n",
            ts_ms, dir, idx, len, head_hex
        );
        // Use blocking write inside the async fn via `tokio::task::spawn_blocking`
        let line_owned = line;
        let path = out_path.clone();
        let _ = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .and_then(|mut f| std::io::Write::write_all(&mut f, line_owned.as_bytes()));
    };

    // Generate ephemeral + identity (x25519-dalek — wacore's KeyPair::generate
    // requires rand 0.10 traits which aren't available in our workspace's rand 0.9)
    let e_secret = StaticSecret::random_from_rng(rand::rngs::OsRng);
    let e_pub_arr: [u8; 32] = *PublicKey::from(&e_secret).as_bytes();
    let e_secret_bytes = e_secret.to_bytes();

    let identity_secret = StaticSecret::random_from_rng(rand::rngs::OsRng);
    let identity_pub_arr: [u8; 32] = *PublicKey::from(&identity_secret).as_bytes();
    let client_payload: Vec<u8> = (0..145).map(|_| rand::random::<u8>()).collect();

    // Open TLS
    let tcp = TcpStream::connect((SERVER_HOST, SERVER_PORT))
        .await
        .with_context(|| format!("TCP {SERVER_HOST}:{SERVER_PORT}"))?;
    tcp.set_nodelay(true).ok();
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let cfg = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(cfg));
    let sn = ServerName::try_from(SERVER_HOST.to_string())?;
    let mut tls = connector.connect(sn, tcp).await.context("TLS")?;

    // WS upgrade
    let key = base64::engine::general_purpose::STANDARD.encode(rand::random::<[u8; 16]>());
    let req = format!(
        "GET /ws/chat HTTP/1.1\r\nHost: {SERVER_HOST}:{SERVER_PORT}\r\n\
         Upgrade: websocket\r\nConnection: Upgrade\r\n\
         Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
    );
    tls.write_all(req.as_bytes()).await?;
    let mut buf = vec![0u8; 4096];
    let n = tls.read(&mut buf).await?;
    let resp = std::str::from_utf8(&buf[..n])?;
    if !resp.starts_with("HTTP/1.1 101") {
        bail!("WS upgrade failed: {}", &resp[..resp.len().min(200)]);
    }
    info!("WS upgrade OK");

    // frame[0]
    let mut frame0 = Vec::with_capacity(43);
    frame0.extend_from_slice(b"WA");
    frame0.extend_from_slice(&[0x06, 0x03, 0x00, 0x00]);
    frame0.extend_from_slice(&[0x24, 0x12, 0x22, 0x0a, 0x20]);
    frame0.extend_from_slice(&e_pub_arr);
    debug_assert_eq!(frame0.len(), 43);
    send_ws_binary_frame(&mut tls, &frame0).await?;
    log("send", 0, frame0.len(), &frame0);

    // frame[1] = server hello
    let frame1 = read_ws_binary_frame(&mut tls).await?;
    log("recv", 1, frame1.len(), &frame1);
    info!("frame[1] len={}", frame1.len());

    let server_hello_protobuf = &frame1[3..];
    let (server_ephemeral, server_static_ct, cert_ct) =
        HandshakeUtils::parse_server_hello(server_hello_protobuf).context("parse_server_hello")?;
    info!(
        "server_eph={}B server_static_ct={}B cert_ct={}B",
        server_ephemeral.len(),
        server_static_ct.len(),
        cert_ct.len()
    );

    // Use wacore's mix_shared_secret (passes raw priv+pub bytes to libsignal)
    let mut noise = NoiseHandshake::new(NOISE_PATTERN_XX, &WA_CONN_HEADER)?;
    noise.authenticate(&e_pub_arr);
    noise.authenticate(&server_ephemeral);
    noise
        .mix_shared_secret(&e_secret_bytes, &server_ephemeral)
        .context("ee")?;
    let server_static_plain = noise.decrypt(&server_static_ct).context("decrypt static")?;
    let server_static_pub: [u8; 32] = server_static_plain
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("server_static not 32B"))?;
    info!(
        "server_static_pub extracted: {}",
        hex::encode(server_static_pub)
    );

    noise
        .mix_shared_secret(&e_secret_bytes, &server_static_pub)
        .context("es")?;
    let _cert = noise.decrypt(&cert_ct).context("decrypt cert")?;

    // frame[2] = client finish = enc(identity_pub) + payload
    let encrypted_pubkey = noise.encrypt(&identity_pub_arr).context("enc pubkey")?;
    noise
        .mix_shared_secret(&identity_secret.to_bytes(), &server_ephemeral)
        .context("se")?;
    let encrypted_payload = noise.encrypt(&client_payload).context("enc payload")?;

    let mut frame2 = Vec::new();
    frame2.extend_from_slice(&encrypted_pubkey);
    frame2.extend_from_slice(&encrypted_payload);
    send_ws_binary_frame(&mut tls, &frame2).await?;
    log("send", 2, frame2.len(), &frame2);
    info!("frame[2] sent ({}B)", frame2.len());

    // frame[3] = server finish / post-handshake init
    match tokio::time::timeout(Duration::from_secs(8), read_ws_binary_frame(&mut tls)).await {
        Ok(Ok(frame3)) => {
            log("recv", 3, frame3.len(), &frame3);
            println!(
                "server frame[3] len={} head={}",
                frame3.len(),
                hex::encode(&frame3[..frame3.len().min(48)])
            );
        }
        Ok(Err(e)) => warn!("recv frame[3] err: {e}"),
        Err(_) => warn!("recv frame[3] timeout (8s)"),
    }

    // After frame[3], the connection is in transport-cipher mode. Subsequent
    // frames would be encrypted. We don't have post-handshake IQ emission code
    // here (that's the daemon's job). Just record what we have and exit.

    println!();
    println!("logged       : {} ({}B)", frame0.len(), 0);
    println!("logged recv1 : {} ({}B)", frame1.len(), 1);
    println!("logged send2 : {} ({}B)", frame2.len(), 2);
    println!("output       : {}", out_path.display());

    let _ = writer.shutdown().await;
    Ok(())
}

async fn send_ws_binary_frame(
    tls: &mut tokio_rustls::client::TlsStream<TcpStream>,
    payload: &[u8],
) -> Result<()> {
    let mut header = vec![0x82u8];
    let len = payload.len();
    if len < 126 {
        header.push(0x80 | len as u8);
    } else if len < 65536 {
        header.push(0x80 | 126);
        header.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        header.push(0x80 | 127);
        header.extend_from_slice(&(len as u64).to_be_bytes());
    }
    let mask: [u8; 4] = rand::random();
    header.extend_from_slice(&mask);
    let mut masked = Vec::with_capacity(len);
    for (i, b) in payload.iter().enumerate() {
        masked.push(b ^ mask[i & 3]);
    }
    tls.write_all(&header).await?;
    tls.write_all(&masked).await?;
    Ok(())
}

async fn read_ws_binary_frame(
    tls: &mut tokio_rustls::client::TlsStream<TcpStream>,
) -> Result<Vec<u8>> {
    let mut assembled: Option<Vec<u8>> = None;
    loop {
        let mut hdr = [0u8; 2];
        tls.read_exact(&mut hdr).await?;
        let fin = hdr[0] & 0x80 != 0;
        let opcode = hdr[0] & 0x0f;
        let masked = hdr[1] & 0x80 != 0;
        let mut len = (hdr[1] & 0x7f) as usize;
        if len == 126 {
            let mut ext = [0u8; 2];
            tls.read_exact(&mut ext).await?;
            len = u16::from_be_bytes(ext) as usize;
        } else if len == 127 {
            let mut ext = [0u8; 8];
            tls.read_exact(&mut ext).await?;
            len = u64::from_be_bytes(ext) as usize;
        }
        let mask = if masked {
            let mut m = [0u8; 4];
            tls.read_exact(&mut m).await?;
            Some(m)
        } else {
            None
        };
        let mut data = vec![0u8; len];
        tls.read_exact(&mut data).await?;
        if let Some(m) = mask {
            for (i, b) in data.iter_mut().enumerate() {
                *b ^= m[i & 3];
            }
        }
        if opcode == 0x2 {
            if let Some(ref mut a) = assembled {
                a.extend_from_slice(&data);
            } else {
                assembled = Some(data);
            }
            if fin {
                return Ok(assembled.unwrap());
            }
        }
    }
}

fn _unused_iv() {
    let _ = generate_iv(0);
}
