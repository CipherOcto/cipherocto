//! RFC 3339 timestamp formatting.
//!
//! R3-H2 / R4-L2: hand-rolled from `SystemTime` + `Duration` to avoid
//! pulling in `chrono` as a direct dep. `chrono` is a transitive dep
//! via `octo-adapter-whatsapp` (which uses it for `Device::pn`
//! timestamps), but using it directly here would create a circular-import
//! risk and an extra dep that we don't need.
//!
//! Mirrors `octo-matrix-onboard/src/logging.rs:82-95` `format_rfc3339_secs`.

use std::time::{SystemTime, UNIX_EPOCH};

/// Format a Unix epoch (seconds since 1970-01-01) as an RFC 3339 UTC
/// string with no sub-second precision: `YYYY-MM-DDTHH:MM:SSZ`.
///
/// Returns `"<unknown>"` for `epoch_secs == 0` so a missing or
/// never-set field doesn't carry a misleading 1969-12-31 timestamp.
///
/// The 20-character output is unit-test-pinned to prevent drift to
/// SQLite-style (`2026-06-12 10:30:00`), epoch-seconds (`1700000000`),
/// or nanosecond-precision (`2026-06-12T10:30:00.123Z`) formats.
pub fn format_rfc3339_secs(epoch_secs: u64) -> String {
    if epoch_secs == 0 {
        return "<unknown>".to_string();
    }
    let (year, month, day) = epoch_days_to_ymd((epoch_secs / 86_400) as i64);
    let hh = (epoch_secs / 3600) % 24;
    let mm = (epoch_secs / 60) % 60;
    let ss = epoch_secs % 60;
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hh, mm, ss
    )
}

/// Get the current wall-clock time as an RFC 3339 UTC string.
/// Convenience wrapper over `format_rfc3339_secs(now)`.
pub fn now_as_rfc3339_secs() -> String {
    let epoch_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_rfc3339_secs(epoch_secs)
}

/// Convert days since 1970-01-01 to (year, month, day) in the proleptic
/// Gregorian calendar. Civil-from-days algorithm from Howard Hinnant's
/// `date` library (public domain).
fn epoch_days_to_ymd(days_since_epoch: i64) -> (i32, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if m <= 2 { y + 1 } else { y } as i32;
    (year, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_rfc3339_secs_zero_returns_unknown() {
        // R5-L2: pre-1970 or unset fields return <unknown>
        assert_eq!(format_rfc3339_secs(0), "<unknown>");
    }

    #[test]
    fn format_rfc3339_secs_known_timestamp() {
        // 1700000000 = 2023-11-14T22:13:20Z (epoch seconds for that moment)
        assert_eq!(format_rfc3339_secs(1_700_000_000), "2023-11-14T22:13:20Z");
    }

    #[test]
    fn format_rfc3339_secs_unix_epoch() {
        // 0 was already tested; 1 = 1970-01-01T00:00:01Z
        assert_eq!(format_rfc3339_secs(1), "1970-01-01T00:00:01Z");
    }

    #[test]
    fn format_rfc3339_secs_format_is_20_chars() {
        // R5-L2: pin the 20-char no-subsec format
        let s = format_rfc3339_secs(1_700_000_000);
        assert_eq!(s.len(), 20);
        assert!(s.ends_with('Z'));
    }

    #[test]
    fn epoch_days_to_ymd_epoch() {
        assert_eq!(epoch_days_to_ymd(0), (1970, 1, 1));
    }

    #[test]
    fn epoch_days_to_ymd_known_date() {
        // Day 20000 after 1970-01-01 is 2024-10-04
        assert_eq!(epoch_days_to_ymd(20_000), (2024, 10, 4));
    }
}
