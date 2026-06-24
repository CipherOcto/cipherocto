use std::sync::Arc;

use clap::Parser;
use octo_network::dot::PlatformType;
use octo_network::sync::TransportBroadcaster;
use octo_sync::adapter::DatabaseSyncAdapter;
use octo_sync::config::{SyncConfig, SyncRole};
use octo_sync::identity::SyncPeerId;
use octo_sync::session::SyncSessionManager;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[derive(Parser)]
#[command(name = "stoolap-node")]
#[command(about = "Minimal Stoolap node for L4 cross-process E2E sync tests")]
struct Args {
    #[arg(short, long)]
    dsn: String,
    #[arg(short, long)]
    listen: u16,
    #[arg(short = 'p', long = "peer")]
    peers: Vec<String>,
    #[arg(long, default_value = "abcd000000000000000000000000000000000000000000000000000000000000")]
    mission_id: String,
    #[arg(long, default_value = "0100000000000000000000000000000000000000000000000000000000000000")]
    node_id: String,
    #[arg(long, default_value = "0")]
    commit: usize,
    #[arg(long)]
    status_file: Option<String>,
    /// Artificial delay (ms) when applying each WAL entry (for backpressure testing).
    #[arg(long, default_value = "0")]
    slow_apply_ms: u64,
    /// Platform adapter to load (e.g., "p2p", "webhook", "quic"). Can be repeated.
    #[arg(long = "adapter")]
    adapters: Vec<String>,
    /// Directories to scan for adapter plugin `.so` files.
    #[arg(long = "adapter-dir")]
    adapter_dirs: Vec<String>,
}

fn parse_hex32(s: &str) -> [u8; 32] {
    let bytes = hex::decode(s).expect("invalid hex");
    assert_eq!(bytes.len(), 32);
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    arr
}

fn adapter_name_to_platform_type(name: &str) -> Option<PlatformType> {
    PlatformType::from_name(name)
}

/// Peer ID sentinel for the transport-based outbound subscriber.
const TRANSPORT_PEER_ID: [u8; 32] = [0xFE; 32];

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "stoolap_node=info".parse().unwrap()),
        )
        .init();

    let args = Args::parse();
    let mission_id = parse_hex32(&args.mission_id);
    let node_id = parse_hex32(&args.node_id);

    let sync_config = stoolap::sync_adapter::SyncConfig::new(mission_id, node_id);
    let (db, adapter) = stoolap::Database::open_with_sync(&args.dsn, sync_config)?;

    if args.commit > 0 {
        tracing::info!(count = args.commit, "committing rows on startup");
        db.execute("CREATE TABLE IF NOT EXISTS sync_test (id INTEGER PRIMARY KEY, data TEXT)", ())
            .expect("failed to create table");
        for i in 0..args.commit {
            let sql = format!("INSERT INTO sync_test (id, data) VALUES ({}, 'row-{}')", i, i);
            db.execute(&sql, ()).expect("failed to insert row");
        }
        tracing::info!(lsn = adapter.current_lsn().unwrap_or(0), "committed rows");
    }

    tracing::info!(listen = %args.listen, peers = ?args.peers, "stoolap-node starting");

    let adapter_arc: Arc<dyn DatabaseSyncAdapter> = adapter;

    // Create SyncSessionManager for the transport path
    let session_config = SyncConfig::new(mission_id, SyncRole::Replicator, vec![0x01; 32]);
    let session = Arc::new(SyncSessionManager::new(
        adapter_arc.clone(),
        session_config,
        &node_id,
    )?);

    // Load platform adapters and create NodeTransport when --adapter is provided
    let transport_peer = SyncPeerId(TRANSPORT_PEER_ID);
    if !args.adapters.is_empty() {
        let plugin_dirs: Vec<std::path::PathBuf> = args.adapter_dirs.iter().map(std::path::PathBuf::from).collect();
        let mut registry = octo_network::dot::adapters::registry::AdapterRegistry::new(plugin_dirs);
        if let Err(e) = registry.discover_and_load() {
            tracing::warn!(errors = ?e, "adapter plugin load errors (continuing with built-in adapters)");
        }

        let requested: Vec<PlatformType> = args.adapters.iter()
            .filter_map(|name| adapter_name_to_platform_type(name))
            .collect();

        let domain = octo_network::dot::BroadcastDomainId::new(
            octo_network::dot::PlatformType::NativeP2P,
            &args.node_id,
        );
        let all_senders = octo_transport::AdapterFactory::from_registry(registry, domain);

        let senders: Vec<Arc<dyn octo_transport::sender::NetworkSender>> = all_senders
            .into_iter()
            .filter(|s| requested.iter().any(|pt| s.name() == pt.name()))
            .collect();

        if !senders.is_empty() {
            let transport = Arc::new(octo_transport::NodeTransport::new(senders));
            tracing::info!(
                transports = transport.transport_count(),
                "NodeTransport wired"
            );
            let broadcaster = Arc::new(
                octo_transport::NodeTransportBroadcaster::new(transport.clone())
                    .with_identity(node_id, node_id)
            );

            // Subscribe the transport peer to the session's outbox
            session.subscribe_peer(transport_peer).unwrap();

            // Spawn the outbox drain task: polls outbox, broadcasts via NodeTransport
            let session_clone = session.clone();
            let broadcaster_clone = broadcaster;
            let mission_id_clone = mission_id;
            let drain_handle = tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_millis(50));
                loop {
                    interval.tick().await;
                    let chunks = session_clone.streamer().drain_outbox(&transport_peer);
                    for chunk in &chunks {
                        let encoded = chunk.encode();
                        match broadcaster_clone.broadcast(&encoded, &mission_id_clone).await {
                            Ok(()) => {
                                tracing::debug!(
                                    from = chunk.from_lsn,
                                    to = chunk.to_lsn,
                                    entries = chunk.entries.len(),
                                    "transport broadcast WAL chunk"
                                );
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "transport broadcast failed");
                            }
                        }
                    }
                }
            });
            // Store handle to abort on shutdown
            drop(drain_handle);
        }
    }

    // TCP sync path (default, backward-compatible)
    let listener = TcpListener::bind(format!("0.0.0.0:{}", args.listen)).await?;
    let adapter_for_accept = adapter_arc.clone();
    let accept_handle = tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    tracing::info!(peer = %addr, "accepted connection");
                    let adapter = adapter_for_accept.clone();
                    tokio::spawn(async move {
                        if let Err(e) = serve_writer(stream, adapter).await {
                            tracing::error!(peer = %addr, error = %e, "connection error");
                        }
                    });
                }
                Err(e) => tracing::error!(error = %e, "accept error"),
            }
        }
    });

    let mut peer_handles = Vec::new();
    for peer_addr in &args.peers {
        let peer = peer_addr.clone();
        let adapter = adapter_arc.clone();
        let db_ref = db.clone();
        let status_file = args.status_file.clone();
        let slow_apply_ms = args.slow_apply_ms;
        let handle = tokio::spawn(async move {
            match TcpStream::connect(&peer).await {
                Ok(stream) => {
                    tracing::info!(peer = %peer, "connected to peer");
                    if let Err(e) = serve_reader(stream, adapter, db_ref, status_file, slow_apply_ms).await {
                        tracing::error!(peer = %peer, error = %e, "peer error");
                    }
                }
                Err(e) => tracing::error!(peer = %peer, error = %e, "failed to connect"),
            }
        });
        peer_handles.push(handle);
    }

    tokio::signal::ctrl_c().await?;
    tracing::info!("shutting down");
    accept_handle.abort();
    for h in peer_handles { h.abort(); }
    Ok(())
}

async fn serve_writer(
    mut stream: TcpStream,
    adapter: Arc<dyn DatabaseSyncAdapter>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut lsn_buf = [0u8; 8];
    stream.read_exact(&mut lsn_buf).await?;
    let request_lsn = u64::from_le_bytes(lsn_buf);
    tracing::info!(request_lsn, "peer requested WAL from");

    let current = adapter.current_lsn()?;
    if current > request_lsn {
        let entries = adapter.read_wal_range(request_lsn + 1, current)?;
        tracing::info!(from = request_lsn + 1, to = current, count = entries.len(), "sending initial WAL batch");
        for entry in &entries {
            let mut frame = Vec::with_capacity(1 + entry.len());
            frame.push(0x01);
            frame.extend_from_slice(entry);
            write_frame(&mut stream, &frame).await?;
        }
    }
    write_frame(&mut stream, &[0x03]).await?;

    let mut last_lsn = current;
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let current = adapter.current_lsn()?;
        if current <= last_lsn { continue; }
        let entries = adapter.read_wal_range(last_lsn + 1, current)?;
        tracing::debug!(from = last_lsn + 1, to = current, count = entries.len(), "sending incremental WAL");
        for entry in &entries {
            let mut frame = Vec::with_capacity(1 + entry.len());
            frame.push(0x01);
            frame.extend_from_slice(entry);
            write_frame(&mut stream, &frame).await?;
        }
        write_frame(&mut stream, &[0x03]).await?;
        last_lsn = current;
    }
}

async fn serve_reader(
    mut stream: TcpStream,
    adapter: Arc<dyn DatabaseSyncAdapter>,
    db: stoolap::Database,
    status_file: Option<String>,
    slow_apply_ms: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut last_lsn = adapter.current_lsn()?;
    stream.write_all(&last_lsn.to_le_bytes()).await?;
    stream.flush().await?;
    tracing::info!(last_lsn, "sent request_lsn to writer");

    loop {
        let len = match read_u32(&mut stream).await {
            Some(l) => l as usize,
            None => { tracing::info!("writer closed connection"); break; }
        };
        if len == 0 || len > 16 * 1024 * 1024 { break; }

        let mut payload = vec![0u8; len];
        stream.read_exact(&mut payload).await?;

        match payload[0] {
            0x01 => {
                if slow_apply_ms > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(slow_apply_ms)).await;
                }
                match adapter.apply_wal_entry(&payload[1..]) {
                    Ok(()) => tracing::debug!("applied WAL entry"),
                    Err(e) => tracing::warn!(error = %e, "failed to apply WAL entry"),
                }
            }
            0x03 => {
                last_lsn = adapter.current_lsn()?;
                tracing::debug!(last_lsn, "batch complete");
                if let Some(ref path) = status_file {
                    let count: i64 = db.query_one("SELECT COUNT(*) FROM sync_test", ())
                        .unwrap_or(-1);
                    let _ = std::fs::write(path, count.to_string());
                    tracing::info!(count, "wrote status file");
                }
            }
            other => { tracing::warn!(msg_type = other, "unknown message type"); }
        }
    }
    Ok(())
}

async fn read_u32(stream: &mut TcpStream) -> Option<u32> {
    let mut buf = [0u8; 4];
    match stream.read_exact(&mut buf).await {
        Ok(_) => Some(u32::from_be_bytes(buf)),
        Err(_) => None,
    }
}

async fn write_frame(stream: &mut TcpStream, data: &[u8]) -> Result<(), std::io::Error> {
    let len = data.len() as u32;
    stream.write_all(&len.to_be_bytes()).await?;
    stream.write_all(data).await?;
    stream.flush().await?;
    Ok(())
}
