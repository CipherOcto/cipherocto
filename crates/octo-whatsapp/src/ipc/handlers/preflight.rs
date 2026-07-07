//! Pre-flight checks for media uploads. Returns a `MediaSlot` or
//! `RpcError` with -32004/-32005/-32006 codes per design §Large outbound
//! media.

use std::path::Path;

use serde_json::json;

use crate::daemon::DaemonHandle;
use crate::ipc::protocol::{RpcError, RpcErrorCode};
use crate::limits::MediaKind;
use crate::media_buffer::{MediaBufferError, MediaSlot};

#[derive(Debug)]
pub struct PreflightOk {
    /// Held by the caller for the duration of the upload. Drop = release.
    pub slot: MediaSlot,
    pub size_bytes: u64,
}

/// Pre-flight for any outbound media RPC. Enforces:
/// 1. file exists and is stat-able (`InvalidParams` on miss),
/// 2. file size <= `kind.max_bytes()` (`PayloadTooLarge` on miss),
/// 3. concurrency cap not saturated (`Busy` on miss),
/// 4. media buffer root is reachable on disk (`DiskUnreachable` on miss).
pub async fn preflight(
    handle: &DaemonHandle,
    kind: MediaKind,
    file: &Path,
) -> Result<PreflightOk, RpcError> {
    let meta = tokio::fs::metadata(file).await.map_err(|e| RpcError {
        code: RpcErrorCode::InvalidParams.as_i32(),
        message: format!("cannot stat {file:?}: {e}"),
        data: None,
    })?;
    let size = meta.len();
    if size > kind.max_bytes() as u64 {
        return Err(RpcError {
            code: RpcErrorCode::PayloadTooLarge.as_i32(),
            message: format!(
                "{:?} payload is {size} bytes; ceiling is {}; use a smaller file or different kind",
                kind,
                kind.max_bytes()
            ),
            data: Some(json!({
                "size_bytes": size,
                "max_bytes": kind.max_bytes(),
                "kind": kind.as_str(),
            })),
        });
    }
    let slot = handle.media_buffer().try_acquire().ok_or(RpcError {
        code: RpcErrorCode::Busy.as_i32(),
        message: "media upload concurrency cap reached; retry shortly".to_string(),
        data: Some(json!({
            "max_concurrent_uploads": handle.media_buffer().max_concurrent(),
        })),
    })?;
    handle
        .media_buffer()
        .check_free_space(size)
        .await
        .map_err(|e| match e {
            MediaBufferError::DiskUnreachable { path } => RpcError {
                code: RpcErrorCode::DiskUnreachable.as_i32(),
                message: format!("media buffer root unreachable: {path:?}"),
                data: None,
            },
        })?;
    Ok(PreflightOk {
        slot,
        size_bytes: size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{EventsConfig, MediaBufferConfig, RulesConfig, SecurityConfig, WhatsAppRuntimeConfig};
    use crate::daemon::Daemon;
    use std::io::Write as _;

    fn handle_with_cap(cap: usize) -> DaemonHandle {
        let cfg = WhatsAppRuntimeConfig {
            name: "pf".into(),
            data_dir: std::env::temp_dir(),
            log_dir: std::env::temp_dir(),
            socket_dir: std::env::temp_dir(),
            media_buffer: MediaBufferConfig {
                max_concurrent_uploads: cap,
                root: std::env::temp_dir().join(format!("octo-pf-{}-{}", std::process::id(), cap)),
            },
            events: EventsConfig::default(),
            security: SecurityConfig::default(),
            observability: Default::default(),
            rules: RulesConfig::default(),
        };
        Daemon::new(cfg).handle()
    }

    #[tokio::test]
    async fn rejects_oversize() {
        let h = handle_with_cap(4);
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("big.bin");
        let mut w = std::fs::File::create(&f).unwrap();
        let chunk = vec![0u8; 1024 * 1024];
        for _ in 0..17 {
            w.write_all(&chunk).unwrap();
        }
        drop(w);

        let err = preflight(&h, MediaKind::Image, &f).await.unwrap_err();
        assert_eq!(err.code, RpcErrorCode::PayloadTooLarge.as_i32());
        let data = err.data.expect("data");
        assert_eq!(data["size_bytes"], 17u64 * 1024 * 1024);
        assert_eq!(data["max_bytes"], MediaKind::Image.max_bytes());
        assert_eq!(data["kind"], "image");
    }

    #[tokio::test]
    async fn rejects_nonexistent_file() {
        let h = handle_with_cap(4);
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("does-not-exist.bin");
        let err = preflight(&h, MediaKind::Audio, &f).await.unwrap_err();
        assert_eq!(err.code, RpcErrorCode::InvalidParams.as_i32());
    }

    #[tokio::test]
    async fn rejects_busy_when_full() {
        let h = handle_with_cap(1);
        // Saturate the buffer by taking the only permit via the same
        // public path the handler uses. The slot lives on the stack and
        // is dropped at the end of the test, releasing the permit.
        let _taken = h.media_buffer().try_acquire().expect("first slot");

        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("ok.bin");
        std::fs::write(&f, [0u8; 8]).unwrap();

        let err = preflight(&h, MediaKind::Sticker, &f).await.unwrap_err();
        assert_eq!(err.code, RpcErrorCode::Busy.as_i32());
        let data = err.data.expect("data");
        assert_eq!(data["max_concurrent_uploads"], 1);
    }
}
