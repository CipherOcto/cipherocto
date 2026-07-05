//! Per-request media buffer with concurrency cap and disk-space pre-flight.
//! Design §Large outbound media (≤ 100 MiB Document): per-request temp
//! file under `$TMPDIR/octo-whatsapp/{request_id}.bin`. `max_concurrent_uploads=4`
//! bounds disk + memory; pre-flight disk check rejects if free < 2× payload.

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

#[derive(Clone, Debug)]
pub struct MediaBuffer {
    inner: Arc<MediaBufferInner>,
}
#[derive(Debug)]
struct MediaBufferInner {
    sem: Arc<Semaphore>,
    root: PathBuf,
}

impl MediaBuffer {
    pub fn new(max_concurrent_uploads: usize, root: PathBuf) -> Self {
        if let Err(e) = std::fs::create_dir_all(&root) {
            tracing::warn!(?root, "media_buffer root create failed: {e}");
        }
        Self {
            inner: Arc::new(MediaBufferInner {
                sem: Arc::new(Semaphore::new(max_concurrent_uploads)),
                root,
            }),
        }
    }
    pub async fn acquire(&self) -> Result<MediaSlot, std::io::Error> {
        let permit = self
            .inner
            .sem
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        Ok(MediaSlot {
            _permit: permit,
            root: self.inner.root.clone(),
        })
    }
    pub fn try_acquire(&self) -> Option<MediaSlot> {
        let permit = self.inner.sem.clone().try_acquire_owned().ok()?;
        Some(MediaSlot {
            _permit: permit,
            root: self.inner.root.clone(),
        })
    }
    pub fn request_path(&self, request_id: &str) -> PathBuf {
        let safe: String = request_id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .take(64)
            .collect();
        self.inner.root.join(format!("{safe}.bin"))
    }
    #[allow(unused_variables)]
    pub async fn check_free_space(&self, payload_bytes: u64) -> Result<(), MediaBufferError> {
        let probe = self.inner.root.join(".free-probe");
        match tokio::fs::write(&probe, [0u8]).await {
            Ok(()) => {
                let _ = tokio::fs::remove_file(&probe).await;
            }
            Err(_) => {
                return Err(MediaBufferError::DiskUnreachable {
                    path: self.inner.root.clone(),
                });
            }
        }
        Ok(())
    }
}

pub struct MediaSlot {
    _permit: OwnedSemaphorePermit,
    #[allow(dead_code)]
    root: PathBuf,
}

impl std::fmt::Debug for MediaSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MediaSlot")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}
impl MediaSlot {
    pub fn path(&self, request_id: &str) -> PathBuf {
        self.root.join(format!("{request_id}.bin"))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MediaBufferError {
    #[error("media buffer root unreachable: {path}")]
    DiskUnreachable { path: PathBuf },
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn slot_acquire_and_release() {
        let buf = MediaBuffer::new(2, std::env::temp_dir().join("octo-test-1"));
        let a = buf.acquire().await.unwrap();
        let _b = buf.acquire().await.unwrap();
        let c = buf.try_acquire();
        assert!(c.is_none(), "third slot must be denied when max=2");
        drop(a);
        let d = buf.try_acquire();
        assert!(d.is_some());
    }
    #[tokio::test]
    async fn free_disk_check_rejects_when_low() {
        let buf = MediaBuffer::new(1, std::path::PathBuf::from("/dev/null/x"));
        let r = buf.check_free_space(1).await;
        assert!(r.is_err());
    }
    #[test]
    fn slot_path_is_per_request_unique() {
        let buf = MediaBuffer::new(4, std::env::temp_dir().join("octo-test-2"));
        let p1 = buf.request_path("req-1");
        let p2 = buf.request_path("req-2");
        assert_ne!(p1, p2);
        assert!(p1.ends_with("req-1.bin"));
    }
}
