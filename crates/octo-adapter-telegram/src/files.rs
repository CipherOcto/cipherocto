//! File upload and download via TDLib.
//!
//! Mission Architecture: "send_envelope / send_file / download_file (TDLib InputFile + downloadFile)"
//!
//! TDLib supports file transfers up to 2 GB, compared to Bot API's 50 MB limit.
//! Uses `inputFile::LocalFile` for uploads and `downloadFile` for downloads.
//!
//! ## Upload Flow
//! 1. Prepare `InputFile::LocalFile` with path and priority
//! 2. Use `sendDocument` or `messages.sendMultiMedia` for large files
//! 3. Track progress via `updateFile` updates
//!
//! ## Download Flow
//! 1. Call `downloadFile` with file_id and desired priority
//! 2. TDLib returns `File` with `local::path` when download completes
//! 3. Read bytes from local path

use crate::error::FileError;
pub use crate::error::FileResult;

#[cfg(feature = "real-tdlib")]
use std::path::Path;
#[cfg(feature = "real-tdlib")]
use tdlib_rs::enums::File;

/// Maximum file size for upload (2 GB per TDLib limit).
pub const MAX_UPLOAD_BYTES: u64 = 2_000_000_000;

/// File metadata from TDLib.
#[cfg(feature = "real-tdlib")]
#[derive(Debug, Clone)]
pub struct FileMetadata {
    pub file_id: i32,
    pub size: u64,
    pub local_path: Option<String>,
    pub remote: Option<RemoteFile>,
}

#[cfg(feature = "real-tdlib")]
#[derive(Debug, Clone)]
pub struct RemoteFile {
    pub id: String,
    pub unique_id: String,
}

// =============================================================================
// File Upload
// =============================================================================

/// Upload a file to Telegram using TDLib.
/// For envelopes (binary data), uses `sendDocument`.
#[cfg(feature = "real-tdlib")]
#[allow(dead_code)] // stub; callers should use `real_client::send_file`
pub(crate) async fn upload_file(
    _client_id: i32,
    _chat_id: i64,
    file_path: &Path,
    _caption: Option<String>,
) -> FileResult<i32> {
    let file_size = std::fs::metadata(file_path)
        .map_err(|e| FileError::ReadError(e.to_string()))?
        .len();

    if file_size > MAX_UPLOAD_BYTES {
        return Err(FileError::TooLarge {
            size: file_size,
            max: MAX_UPLOAD_BYTES,
        });
    }

    // TODO: Implement actual TDLib sendDocument via observer pattern.
    // The actual implementation requires proper async observe setup.
    // In the meantime, real_client::send_file does the upload directly
    // and bypasses this stub.
    Err(FileError::Unimplemented(
        "upload_file: use real_client::send_file".into(),
    ))
}

/// Upload raw bytes as a document (for DOT envelopes).
#[cfg(feature = "real-tdlib")]
#[allow(dead_code)] // stub; callers should use `real_client::send_file`
pub(crate) async fn upload_bytes(
    client_id: i32,
    chat_id: i64,
    _filename: &str,
    data: &[u8],
) -> FileResult<i32> {
    use std::io::Write;

    let temp_path = unique_temp_path("octo_envelope");
    {
        let mut file =
            std::fs::File::create(&temp_path).map_err(|e| FileError::WriteError(e.to_string()))?;
        file.write_all(data)
            .map_err(|e| FileError::WriteError(e.to_string()))?;
    }

    let result = upload_file(client_id, chat_id, &temp_path, None).await;

    crate::cleanup::cleanup_temp_file(&temp_path);

    result
}

#[cfg(feature = "real-tdlib")]
impl From<std::io::Error> for FileError {
    fn from(e: std::io::Error) -> Self {
        FileError::ReadError(e.to_string())
    }
}

// =============================================================================
// File Download
// =============================================================================

/// Download a file by its TDLib file_id.
/// Returns the local path when download completes.
#[cfg(feature = "real-tdlib")]
pub async fn download_file(client_id: i32, file_id: i32, priority: i32) -> FileResult<String> {
    let file = tdlib_rs::functions::download_file(file_id, priority, 0, 0, true, client_id)
        .await
        .map_err(|e| FileError::Tdlib { message: e.message })?;

    match file {
        File::File(f) => {
            if !f.local.path.is_empty() {
                return Ok(f.local.path);
            }
            Err(FileError::DownloadFailed(
                "file not locally available, download not yet implemented".into(),
            ))
        }
    }
}

/// Download file and return bytes.
#[cfg(feature = "real-tdlib")]
pub async fn download_file_bytes(client_id: i32, file_id: i32) -> FileResult<Vec<u8>> {
    let local_path = download_file(client_id, file_id, 32).await?;
    std::fs::read(&local_path).map_err(|e| FileError::ReadError(e.to_string()))
}

// =============================================================================
// File Progress Tracking
// =============================================================================

/// File download progress update.
#[cfg(feature = "real-tdlib")]
#[derive(Debug, Clone)]
pub struct FileProgress {
    pub file_id: i32,
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    pub is_complete: bool,
}

/// Parse updateFile for progress tracking.
#[cfg(feature = "real-tdlib")]
pub fn parse_file_progress(update: &tdlib_rs::enums::Update) -> Option<FileProgress> {
    match update {
        tdlib_rs::enums::Update::File(update) => Some(FileProgress {
            file_id: update.file.id,
            bytes_downloaded: update.file.size as u64,
            total_bytes: update.file.size as u64,
            is_complete: !update.file.local.path.is_empty(),
        }),
        _ => None,
    }
}

/// Generate a unique temp file path under the system temp dir.
#[cfg(feature = "real-tdlib")]
#[allow(dead_code)] // `real_client` has its own copy; kept for symmetry
pub(crate) fn unique_temp_path(prefix: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("{}_{}_{}", prefix, std::process::id(), id))
}
