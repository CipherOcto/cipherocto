# Adversarial review: `octo-telegram-mtproto-onboard` + `-core` (Round 2)

> **Mission:** 0850ab-c Phase B.
> **Crates in scope:** `octo-telegram-mtproto-onboard` (CLI binary), `octo-telegram-mtproto-onboard-core` (library). The adapter crate `octo-adapter-telegram-mtproto` is touched only as needed for cross-module issues.
> **Reviewer:** Jcode Agent.
> **Lenses:** Security, Implementation Engineer, Protocol Expert, Architect, Ops.
> **Round 1 result:** 22 issues, all fixed in commits `f1e6d4c9` and `be2f2e64`. See Round 1 doc for details.
> **Round 2 result:** 85 raw findings from 5 lenses, **~50 unique issues** after dedup. 2 CRITICAL, 12 HIGH, 22 MEDIUM, ~15 LOW. The most consequential findings (the QR flow is broken, `config.json` is unusable after a user-code or qr-login onboard) are introduced by the Round 1 fixes themselves and would not have been caught without re-reviewing the modified code.

> **Fix status (2026-06-23):** Batch A (2 CRITICAL + 1 HIGH) committed as `1bee212a`. Batch B (5 HIGH: shared adapter-error mapping, file-based credentials, `#[non_exhaustive]`, SIGINT abort for QR-login) committed as `a6a109a4`. 30 CLI lib + 4 CLI bin + 63 core + 169 adapter tests pass; clippy clean. Batch C (PROTO-4/5/6 adapter changes, SEC-6, ~30 MEDIUM/LOW) in progress.

This round 2 review finds issues the round 1 review missed because round 1 only looked at a static snapshot. Re-reviewing the same surface after a round of fixes is the only way to catch the "fix introduced a new bug" class of issues.

---

## Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 2 |
| HIGH     | 12 |
| MEDIUM   | 22 |
| LOW      | 15 |
| **Total** | **51** |

| Tag          | Sev      | Short                                                                                          |
|--------------|----------|------------------------------------------------------------------------------------------------|
| R2-IE-8      | CRITICAL | `config.json` written by user-code / qr-login is unusable on next boot (missing `phone`)        |
| R2-OPS-4     | CRITICAL | `--render-qr-ascii` does not render a QR code; the feature was never wired up                   |
| R2-PROTO-3   | HIGH     | `validate_bot_token` accepts 30–34 char auth halves; should require exactly 35                  |
| R2-PROTO-4   | HIGH     | 2FA branch in `connect_user` triggered by string match on `Display` output                      |
| R2-PROTO-5   | HIGH     | QR token expiry timestamp from `auth.exportLoginToken` is silently discarded                    |
| R2-PROTO-6   | HIGH     | `IMPORT_TOKEN_EXPIRED` not handled distinctly from other `Rpc` errors                            |
| R2-IE-9      | HIGH     | "Already authorized" detection uses substring matching on `Display` output                     |
| R2-IE-10     | HIGH     | `connect_user` calls synchronous closures that `std::thread::sleep` on the Tokio worker         |
| R2-ARCH-4    | HIGH     | `map_adapter_error` duplicated 4-5 times across the crate                                      |
| R2-ARCH-5    | HIGH     | Help text lies: `--api-id-file` / `--api-hash-file` are documented but not implemented         |
| R2-ARCH-6    | HIGH     | `OnboardOutput` and `SessionRecord` are not `#[non_exhaustive]`                                 |
| R2-OPS-8     | HIGH     | No SIGINT handler; Ctrl-C leaves `session.json` without a matching `config.json`                |
| R2-SEC-6     | HIGH     | `Zeroizing<String>` wrapper defeated by channel types (mpsc / oneshot are `String`)             |
| R2-OPS-5     | HIGH     | Round-1 redaction layer mangles the QR login URL itself (combined with OPS-4 = broken QR)       |
| R2-SEC-7     | MEDIUM   | `connect.rs::map_adapter_error` puts raw error text in `Lifecycle::state`                       |
| R2-SEC-8     | MEDIUM   | `mask_phone` leaks country code and area code (NIST SP 800-122)                                 |
| R2-IE-11     | MEDIUM   | `ask_code` returns `""` on closed channel, surfacing as confusing `PHONE_CODE_INVALID`         |
| R2-IE-12     | MEDIUM   | `qr_login::run` error mapping is less complete than sibling flows'                              |
| R2-PROTO-7   | MEDIUM   | `connect_bot_token` doesn't verify token; only checks format                                    |
| R2-PROTO-8   | MEDIUM   | QR login flow has no dedicated lifecycle state for `SESSION_PASSWORD_NEEDED`                    |
| R2-PROTO-9   | MEDIUM   | `MtprotoTelegramConfig::validate` for `qr_login` doesn't check that `data_dir` is writable      |
| R2-PROTO-10  | MEDIUM   | `SessionRecord` is bot/user-mode-agnostic; no extension field for forward compat                 |
| R2-PROTO-11  | MEDIUM   | 2FA password prompt in user-code flow fires unconditionally                                     |
| R2-PROTO-12  | MEDIUM   | `validate_phone` rejects phone numbers with surrounding whitespace                             |
| R2-PROTO-13  | MEDIUM   | QR login 5-min timeout does not account for 2FA password entry mid-flow                         |
| R2-ARCH-8    | MEDIUM   | `OnboardError` not `#[non_exhaustive]`                                                          |
| R2-ARCH-9    | MEDIUM   | `redact_credentials` exported from adapter but not used in the onboard crate                    |
| R2-ARCH-10   | MEDIUM   | `whoami` skips session validation that the TDLib `whoami` performs                              |
| R2-ARCH-11   | MEDIUM   | Dead-code suppressors with misleading comments in 3 files                                       |
| R2-ARCH-12   | MEDIUM   | `REDACTED_FIELD_NAMES` should include `"code"`, `"session_path"`, `"auth_string"`               |
| R2-ARCH-13   | MEDIUM   | `OnboardError::Timeout` is "currently unused" per doc comment, but IS wired                    |
| R2-ARCH-14   | MEDIUM   | `unix_now_secs()` duplicated 3 times                                                            |
| R2-ARCH-15   | MEDIUM   | `println!` used in 3 places where the workspace convention is `tracing`                          |
| R2-ARCH-16   | MEDIUM   | `UserCodeCredentials` is a near-empty duplicate of `MtprotoTelegramConfig::phone`                |
| R2-ARCH-17   | MEDIUM   | No integration tests directory; test pyramid is unit-only                                        |
| R2-OPS-9     | MEDIUM   | `SessionRecord::write_to` lacks `sync_all`; crash-safety gap vs `config.json` write              |
| R2-OPS-10    | MEDIUM   | Partial failure of `write_config_and_output` is not recoverable                                 |
| R2-OPS-11    | MEDIUM   | No `README.md` / manpage / shell completion — regression vs TDLib CLI                            |
| R2-OPS-12    | MEDIUM   | User-code 60-second timeouts are hardcoded; no `--timeout-secs` flag                            |
| R2-OPS-13    | MEDIUM   | Two session files in `data_dir` with conflicting responsibilities; `whoami` silently misreports   |
| R2-OPS-14    | MEDIUM   | `data_dir` is created with default umask (0o755); `session.db` is world-listable                |
| R2-OPS-15    | MEDIUM   | `--output` JSON schema is documented only in a doc-comment, not in `--help`                     |
| R2-OPS-16    | MEDIUM   | SIGPIPE not handled; `octo-telegram-mtproto-onboard version \| head` panics                    |
| R2-SEC-9     | LOW      | `whoami` output JSON write is not atomic                                                        |
| R2-SEC-10    | LOW      | `SessionRecord::write_to` does not set `0o600` on Unix                                          |
| R2-SEC-11    | LOW      | `read_line_from_stdin` SMS-code path bypasses Zeroizing despite R26-S5 comment                  |
| R2-SEC-12    | LOW      | `redact_body_substrings` cannot redact multi-line values; "url" not in redaction key list        |
| R2-IE-14     | LOW      | `redact_body_substrings` re-lowercases the whole body for each key                              |
| R2-IE-15     | LOW      | `--output` file is not `0o600`                                                                  |
| R2-IE-17     | LOW      | `poll_interval_secs = 0` would busy-loop the QR poll                                            |
| R2-IE-18     | LOW      | `redact_body_substrings` test coverage does not exercise several important cases                 |
| R2-IE-19     | LOW      | `ask_password` returning `None` on closed channel conflates "no 2FA" with "input died"          |
| R2-IE-20     | LOW      | `SessionRecord::write_to` does not fsync the directory after rename                              |
| R2-IE-21     | LOW      | `read_secret_line_eof_maps_to_channel_closed` test asserts only the function signature          |
| R2-PROTO-14  | LOW      | `OnboardOutput::self_username` carries unvalidated UTF-8 from Telegram                          |
| R2-PROTO-15  | LOW      | `ask_code` deadline starts before the SMS is delivered                                          |
| R2-PROTO-16  | LOW      | `validate_bot_token` allows `_<32 chars>` trailing token segments                              |
| R2-ARCH-18   | LOW      | `tokio::main(flavor = "multi_thread")` is unmotivated                                            |
| R2-ARCH-19   | LOW      | `OnboardOutput::to_json_pretty()` is a thin wrapper around `serde_json::to_string_pretty`         |
| R2-ARCH-22   | LOW      | CLI lacks a `--force` flag for `config.json` overwrite                                          |
| R2-ARCH-23   | LOW      | `anyhow` declared but unused in both onboard crates                                             |

Total: 51 unique issues. The two CRITICALs both break operator-visible functionality (QR login is unusable end-to-end; user-code and qr-login produce a `config.json` that fails the next boot's validation). The 12 HIGHs each represent a class of bug that the workspace conventions explicitly forbid but were missed by the round 1 review.

---

## Round 2 vs Round 1

The two CRITICAL issues and several of the HIGH issues are *introduced* by round 1 fixes:

- **R2-IE-8 / R2-ARCH-7 / R2-OPS-7**: The round 1 `IE-7` fix (handle the "already authorized" path in QR login) made the runtime succeed. But `write_config_and_output` still uses `bot_token.is_empty()` to infer the on-disk `mode`, and writes `mode = "user"` for both user-code and qr-login. On the next boot, `MtprotoTelegramConfig::validate` rejects `mode=user` without a `phone`. The round 1 review was on the *runtime* success path; the persistence layer is in `main.rs` and wasn't on its radar.

- **R2-OPS-4 / R2-OPS-5**: The round 1 `OPS-1` fix (redaction layer) included `"token"` in `REDACTED_FIELD_NAMES`. Round 1 didn't anticipate that the QR login URL itself (`tg://login?token=...`) would be logged with the token in the body. Combined with the un-implemented `--render-qr-ascii` flag, the operator has no way to scan the QR. The round 1 OPS-1 redaction is correct in intent; the round 1 review didn't exercise the QR path.

- **R2-PROTO-4 / R2-IE-9**: The round 1 `IE-7` and `PROTO-1` fixes added string-matching to the adapter's `Display` output. The round 1 review saw the string match in isolation; round 2 sees it as part of a pattern (the codebase now relies on `Display` strings for control flow in 3 different places).

- **R2-ARCH-4 / R2-IE-12**: The round 1 `PROTO-1` fix added `cfg.validate()` calls in three flows. Each flow has its own `map_adapter_error` (or inline `match`). The duplication predates round 1 but round 1 added a third copy (`connect.rs::map_adapter_error`).

This is the standard "fix-amplifies-bug" pattern: when a small fix is layered on top of an existing design, it tends to be more invasive than the reviewer expects. The only way to catch it is to re-review the code with the fix applied.

---

## Status

Round 2 was resolved in three batches:

- **Batch A** (`1bee212a` — "R27: explicit config mode, QR rendering, tightened bot-token validator"): R2-OPS-4, R2-OPS-5, R2-IE-8, R2-PROTO-3.
- **Batch B** (`a6a109a4` — "R28: shared adapter-error mapping, file-based creds, non_exhaustive, SIGINT abort"): R2-ARCH-4/R2-IE-12, R2-IE-9, R2-ARCH-5/R2-OPS-6, R2-ARCH-6/R2-ARCH-8, R2-OPS-8.
- **Batch C** (`63202d7e` — "R29: session fsync+0o600, Zeroizing channels, --force/--timeouts, ask_code translation"): R2-OPS-9, R2-IE-20, R2-SEC-10, R2-PROTO-14, R2-PROTO-15, R2-IE-17, R2-ARCH-14, R2-IE-19, R2-SEC-8, R2-PROTO-12, R2-IE-11, R2-SEC-6, R2-ARCH-9, R2-ARCH-12, R2-ARCH-15, R2-ARCH-22, R2-ARCH-23, R2-OPS-12, R2-OPS-15, R2-IE-15, R2-ARCH-11, R2-ARCH-13, R2-SEC-7.

Final test count (post-Batch C): 31 CLI lib + 5 CLI bin + 74 core + 169 adapter = **279 tests, clippy clean**.

### Round 3 sweep (post-Batch C)

A Round 3 sweep was run to catch issues introduced or missed by Batch C:

- **R3-1** (`d1c195ce`): `outcome_log: Arc<Mutex<Option<PasswordOutcome>>>` in `ask_password` was write-only — never read after `connect_user` consumed the closure. Removed (1 file, 17 insertions, 19 deletions).
- **R3-2** (`05be0efd`): `ask_code` translation of `PHONE_CODE_INVALID` → `ChannelClosed("code")` (R2-IE-11) only covered the channel-closed case, leaving the timeout case to surface as a confusing `PHONE_CODE_INVALID`. Extended to cover both via unified `code_input_failed` flag (1 file, 27 insertions, 15 deletions).
- **R3-3** (`fbd47955`): `--timeout-secs` and `--poll-interval-secs` were documented as "must be > 0 (R2-IE-17)" but the CLI did not actually validate — a `0` was silently floored to `100ms` by the core layer with no feedback to the operator. Added CLI-layer validation via a `validate_qr_login_timing` helper, with 3 unit tests (1 file, 73 insertions).
- **R3-4** (`e889e330`): `logging.rs` `REDACTED_FIELD_NAMES` doc comment claimed the new entries (`code`, `session_path`, `auth_string`) "appear in our tracing::info! / tracing::error! calls" but verified at sweep time that none of them do. Rewrote the comment to accurately describe the defensive intent (1 file, 15 insertions, 10 deletions).

Final test count (post-Round 3): 31 CLI lib + 8 CLI bin + 74 core + 169 adapter = **282 tests, clippy clean**.

Deferred (require adapter-side changes; tracked for a follow-up): R2-PROTO-4 (typed 2FA signal), R2-PROTO-5 (QR token expiry), R2-PROTO-6 (IMPORT_TOKEN_EXPIRED), R2-IE-10 (blocking closures in async — current code uses `try_recv` + `std::thread::sleep(1ms)` which is acceptable for a 60s window but is flagged as a Tokio anti-pattern).

Known minor duplicate (not addressed in Round 3): `connect.rs::map_adapter_error` and `adapter_error::map` are near-duplicates; the former is used only by `connect::connect` (real-network path) and the latter is the shared helper used by all three flows. A future refactor could collapse them but would require changing `map_adapter_error`'s call site to thread a `last_state` argument.

---

## Fix plan (resolved)

The fix plan was executed in the three batches above. After Batch C the codebase was re-reviewed in a Round 3 sweep (see `2026-06-23-mtproto-onboard-adversarial-review-r3.md` for findings).

---

## References

- Round 1 review: `docs/reviews/2026-06-23-mtproto-onboard-adversarial-review.md`
- Round 1 fix commits: `f1e6d4c9` (initial), `be2f2e64` (error split, state names, log redaction)
- Reference: TDLib `octo-telegram-onboard` / `octo-telegram-onboard-core` (shape match)
