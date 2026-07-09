//! Integration tests for the events disk persister (Phase 3 Part D).
//!
//! Exercises the full `EventsPersisterHandle` actor loop: push
//! events, observe them on disk, restart with a fresh actor + new
//! buffer, assert continuity. Also covers disabled mode, flush_sync
//! semantics, shutdown drain, and the concurrent-push-+-reload race
//! window.
//!
//! Each test uses its own `tempfile::TempDir` so there is no
//! cross-test contamination; `cargo test` reuses paths under
//! `$CARGO_TARGET_TMPDIR` which is itself hermetic.
//!
//! The actor only exits on `cancel` (the daemon's shutdown signal).
//! Tests MUST call `token.cancel()` before `handle.join()` — calling
//! `join` without cancelling deadlocks the test forever.

use std::time::Duration;

use octo_whatsapp::events::InboundEvent;
use octo_whatsapp::events_buffer::EventsBuffer;
use octo_whatsapp::events_persister::{
    default_persistence_path, load_initial_events, EventsPersisterHandle, PersistedEvent,
};
use tokio_util::sync::CancellationToken;

fn dummy_event(tag: &str) -> InboundEvent {
    InboundEvent::Unknown {
        raw: tag.to_string(),
        ts_unix_ms: 0,
        ts_mono_ns: 0,
        untrusted: false,
    }
}

fn new_token() -> CancellationToken {
    CancellationToken::new()
}

async fn shutdown(handle: EventsPersisterHandle, token: CancellationToken) {
    token.cancel();
    handle.join().await.expect("join");
}

#[tokio::test]
async fn append_then_reload_round_trips() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = default_persistence_path(dir.path());
    let buffer = EventsBuffer::new(100);
    let token = new_token();
    let handle = EventsPersisterHandle::spawn(
        buffer.clone(),
        Some(path.clone()),
        Duration::from_millis(50),
        token.clone(),
    )
    .expect("spawn");

    for i in 0..3 {
        handle.push(dummy_event(&format!("m{i}"))).expect("push");
    }
    handle.flush_sync(Duration::from_secs(2)).await.expect("flush");
    shutdown(handle, token).await;

    // Sanity: file should now have 3 lines.
    let bytes = std::fs::read(&path).expect("read");
    let line_count = std::str::from_utf8(&bytes)
        .expect("utf8")
        .split_terminator('\n')
        .count();
    assert_eq!(line_count, 3, "first actor must leave 3 lines on disk");

    // New buffer + new actor, same path. Reload should hydrate.
    let buffer2 = EventsBuffer::new(100);
    let token2 = new_token();
    let _handle2 = EventsPersisterHandle::spawn(
        buffer2.clone(),
        Some(path),
        Duration::from_millis(50),
        token2.clone(),
    )
    .expect("spawn 2");

    assert_eq!(buffer2.len(), 3, "all 3 events must reload");
    // Next push continues ids from 4, not 1.
    let next_id = buffer2.push(dummy_event("new"));
    assert_eq!(next_id, 4, "id sequence must continue post-reload");
    // No need to shutdown _handle2; the buffer alone is enough for
    // the assertion. Cancel for cleanliness.
    token2.cancel();
}

#[tokio::test]
async fn append_writes_one_ndjson_line_per_event() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = default_persistence_path(dir.path());
    let buffer = EventsBuffer::new(100);
    let token = new_token();
    let handle = EventsPersisterHandle::spawn(
        buffer.clone(),
        Some(path.clone()),
        Duration::from_millis(50),
        token.clone(),
    )
    .expect("spawn");

    for i in 0..5 {
        handle.push(dummy_event(&format!("m{i}"))).expect("push");
    }
    handle.flush_sync(Duration::from_secs(2)).await.expect("flush");

    let bytes = std::fs::read(&path).expect("read");
    let content = std::str::from_utf8(&bytes).expect("utf8");
    let lines: Vec<&str> = content.split_terminator('\n').collect();
    assert_eq!(lines.len(), 5, "exactly 5 NDJSON lines");
    for line in &lines {
        let v: serde_json::Value = serde_json::from_str(line).expect("valid json");
        assert!(v.get("id").is_some());
        assert!(v.get("event").is_some());
    }
    assert!(bytes.last() == Some(&b'\n') || bytes.is_empty());

    shutdown(handle, token).await;
}

#[tokio::test]
async fn reload_truncates_partial_trailing_line() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = default_persistence_path(dir.path());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    // Manually write 2 valid lines + a partial trailing line.
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)
        .expect("open");
    use std::io::Write;
    for i in 0..2 {
        let pe = PersistedEvent {
            id: i + 1,
            ts_unix_ms: i,
            ts_mono_ns: i,
            event: dummy_event(&format!("m{i}")),
        };
        serde_json::to_writer(&mut f, &pe).expect("encode");
        writeln!(&mut f).expect("newline");
    }
    f.write_all(b"{\"id\":3,\"ev").expect("partial");
    drop(f);
    let before_len = std::fs::metadata(&path).expect("stat").len();

    let buffer = EventsBuffer::new(100);
    let stats = load_initial_events(&path, &buffer).await.expect("load");
    assert_eq!(stats.loaded, 2);
    assert!(stats.dropped_partial_bytes > 0);
    assert_eq!(buffer.len(), 2);

    let after_len = std::fs::metadata(&path).expect("stat").len();
    assert!(after_len <= before_len, "truncation must shrink file");
}

#[tokio::test]
async fn reload_skips_malformed_middle_lines() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = default_persistence_path(dir.path());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)
        .expect("open");
    use std::io::Write;
    for i in 0..3 {
        let pe = PersistedEvent {
            id: i + 1,
            ts_unix_ms: i,
            ts_mono_ns: i,
            event: dummy_event(&format!("good{i}")),
        };
        serde_json::to_writer(&mut f, &pe).expect("encode");
        writeln!(&mut f).expect("newline");
    }
    writeln!(&mut f, "{{not valid json").expect("garbage");
    for i in 3..5 {
        let pe = PersistedEvent {
            id: i + 1,
            ts_unix_ms: i,
            ts_mono_ns: i,
            event: dummy_event(&format!("good{i}")),
        };
        serde_json::to_writer(&mut f, &pe).expect("encode");
        writeln!(&mut f).expect("newline");
    }
    drop(f);

    let buffer = EventsBuffer::new(100);
    let stats = load_initial_events(&path, &buffer).await.expect("load");
    assert_eq!(stats.loaded, 5);
    assert_eq!(stats.skipped_malformed, 1);
    assert_eq!(buffer.len(), 5);
}

#[tokio::test]
async fn reload_assigns_next_id_after_max() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = default_persistence_path(dir.path());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)
        .expect("open");
    use std::io::Write;
    // Persist with sparse ids 1..=5 (no gaps).
    for i in 0..5 {
        let pe = PersistedEvent {
            id: i + 1,
            ts_unix_ms: i,
            ts_mono_ns: i,
            event: dummy_event(&format!("m{i}")),
        };
        serde_json::to_writer(&mut f, &pe).expect("encode");
        writeln!(&mut f).expect("newline");
    }
    drop(f);

    let buffer = EventsBuffer::new(100);
    load_initial_events(&path, &buffer).await.expect("load");
    let next_id = buffer.push(dummy_event("after"));
    assert_eq!(next_id, 6);
}

#[tokio::test]
async fn eviction_to_disk_keeps_append_only_log() {
    // The disk log is APPEND-ONLY: the actor writes every event as
    // it arrives. Eviction happens only in the in-memory buffer's
    // bounded ring. After reload, the fresh buffer hydrates from the
    // log file (NOT respecting the previous buffer's max_rows, since
    // we control it via the *new* buffer's max_rows at hydrate
    // time). The log itself is the durable record.
    //
    // Verifies:
    //   - Disk has 10 lines (every event we pushed).
    //   - Reload into a 100-row buffer hydrates all 10.
    //   - next_id continues from 11.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = default_persistence_path(dir.path());
    let buffer = EventsBuffer::new(3); // tight in-memory cap
    let token = new_token();
    let handle = EventsPersisterHandle::spawn(
        buffer.clone(),
        Some(path.clone()),
        Duration::from_millis(50),
        token.clone(),
    )
    .expect("spawn");

    for i in 0..10 {
        handle.push(dummy_event(&format!("m{i}"))).expect("push");
    }
    handle.flush_sync(Duration::from_secs(2)).await.expect("flush");
    shutdown(handle, token).await;

    // Disk log: all 10 events written.
    let bytes = std::fs::read(&path).expect("read");
    let content = std::str::from_utf8(&bytes).expect("utf8");
    let lines: Vec<&str> = content.split_terminator('\n').collect();
    assert_eq!(lines.len(), 10, "append-only log has every event");
    // First id should be 1.
    let first: serde_json::Value = serde_json::from_str(lines[0]).expect("json");
    assert_eq!(first["id"].as_u64().expect("id"), 1);

    // Reload into a fresh buffer with bigger cap. All 10 should
    // hydrate (the file is the source of truth, not the previous
    // buffer's eviction).
    let buffer2 = EventsBuffer::new(100);
    load_initial_events(&path, &buffer2).await.expect("reload");
    assert_eq!(buffer2.len(), 10);
    let next = buffer2.push(dummy_event("after"));
    assert_eq!(next, 11);
}

#[tokio::test]
async fn persistence_disabled_creates_no_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = default_persistence_path(dir.path());
    assert!(!path.exists());

    let buffer = EventsBuffer::new(100);
    let token = new_token();
    let handle = EventsPersisterHandle::spawn(
        buffer.clone(),
        None, // disabled
        Duration::from_millis(50),
        token.clone(),
    )
    .expect("spawn");

    for i in 0..100 {
        handle.push(dummy_event(&format!("m{i}"))).expect("push");
    }
    handle.flush_sync(Duration::from_secs(2)).await.expect("flush");
    shutdown(handle, token).await;

    assert!(!path.exists(), "no file must be created when disabled");
    assert_eq!(buffer.len(), 100);
}

#[tokio::test]
async fn flush_sync_blocks_until_disk() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = default_persistence_path(dir.path());
    let buffer = EventsBuffer::new(100);
    let token = new_token();
    let handle = EventsPersisterHandle::spawn(
        buffer.clone(),
        Some(path.clone()),
        // Long flush interval: forces flush_sync to be the one
        // that actually pushes bytes to disk.
        Duration::from_secs(60),
        token.clone(),
    )
    .expect("spawn");

    handle.push(dummy_event("one")).expect("push");
    // Before flush_sync, the file may exist (actor creates it) but
    // be empty (no fsync pushed bytes through the page cache).
    let before = std::fs::read(&path).map(|b| b.len()).unwrap_or(0);
    handle.flush_sync(Duration::from_secs(2)).await.expect("flush");
    let after = std::fs::read(&path).expect("read after").len();
    assert!(after > before, "flush_sync must grow the file");
    shutdown(handle, token).await;
}

#[tokio::test]
async fn shutdown_drain_writes_pending() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = default_persistence_path(dir.path());
    let buffer = EventsBuffer::new(100);
    let token = new_token();
    let handle = EventsPersisterHandle::spawn(
        buffer.clone(),
        Some(path.clone()),
        // Long ticker: forces shutdown drain to do the writing.
        Duration::from_secs(60),
        token.clone(),
    )
    .expect("spawn");

    for i in 0..5 {
        handle.push(dummy_event(&format!("m{i}"))).expect("push");
    }
    // Trigger cancel; the actor must drain the rx and write all
    // pending events before exit.
    shutdown(handle, token).await;

    // Reload to verify all 5 made it to disk.
    let buffer2 = EventsBuffer::new(100);
    let stats = load_initial_events(&path, &buffer2).await.expect("reload");
    assert_eq!(stats.loaded, 5, "drain must persist all 5");
    assert_eq!(buffer2.len(), 5);
}

#[tokio::test]
async fn concurrent_push_and_reload_safe() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = default_persistence_path(dir.path());
    let buffer = EventsBuffer::new(1000);
    let token = new_token();
    let handle = EventsPersisterHandle::spawn(
        buffer.clone(),
        Some(path.clone()),
        Duration::from_millis(20),
        token.clone(),
    )
    .expect("spawn");

    let h = handle;
    let push_token = token.clone();
    let push_task = tokio::spawn(async move {
        for i in 0..10 {
            h.push(dummy_event(&format!("bg{i}"))).expect("push");
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        (h, push_token)
    });
    // Give the actor time to flush some.
    tokio::time::sleep(Duration::from_millis(60)).await;
    // Reload via a separate read while writes are still pending.
    let buffer2 = EventsBuffer::new(1000);
    let stats = load_initial_events(&path, &buffer2).await.expect("reload");
    assert!(stats.loaded <= 10, "no more than what was written");
    let next_id = buffer2.push(dummy_event("after"));
    let expected_min = if stats.loaded > 0 {
        stats.loaded + 1
    } else {
        1
    };
    assert!(next_id >= expected_min, "next_id must continue from max");

    // Wait for the background pusher to finish; join.
    let (handle, push_token) = push_task.await.expect("join");
    handle.flush_sync(Duration::from_secs(2)).await.expect("flush");
    shutdown(handle, push_token).await;

    // Final reload: 10 events present.
    let buffer3 = EventsBuffer::new(1000);
    let stats3 = load_initial_events(&path, &buffer3).await.expect("reload 2");
    assert_eq!(stats3.loaded, 10, "all 10 events must end up on disk");
    // Don't leak the original token.
    token.cancel();
}

#[tokio::test]
async fn dropped_counter_api_works() {
    // We don't force a backpressure drop in a unit test (would
    // require pathological setup); we just verify the public API
    // is exposed and returns a value.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = default_persistence_path(dir.path());
    let buffer = EventsBuffer::new(100);
    let token = new_token();
    let handle = EventsPersisterHandle::spawn(
        buffer.clone(),
        Some(path),
        Duration::from_millis(50),
        token.clone(),
    )
    .expect("spawn");
    for i in 0..100 {
        handle.push(dummy_event(&format!("m{i}"))).expect("push");
    }
    let _dropped = handle.dropped_total();
    shutdown(handle, token).await;
}
