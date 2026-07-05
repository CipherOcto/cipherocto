//! Smoke test: `octo-whatsapp <unknown>` must fail with clap's usage-error
//! exit code (2). Catches regressions where clap silently swallows unknown
//! subcommands or returns 0/1 instead of the documented 2.

#[test]
fn cli_unknown_subcommand_exits_non_zero() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_octo-whatsapp"))
        .arg("this-is-not-a-real-subcommand")
        .output()
        .expect("failed to spawn CLI");

    assert!(
        !output.status.success(),
        "expected non-zero exit for unknown subcommand, got status: {:?}\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    // clap returns exit code 2 for usage errors (unknown subcommand).
    assert_eq!(
        output.status.code(),
        Some(2),
        "expected exit code 2 from clap usage error, got: {:?}",
        output.status.code()
    );

    // Sanity: clap prints an error to stderr naming the bad subcommand.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("this-is-not-a-real-subcommand")
            || stderr.to_lowercase().contains("unrecognized")
            || stderr.to_lowercase().contains("invalid"),
        "expected clap error mentioning the bad subcommand, got stderr: {stderr}"
    );
}