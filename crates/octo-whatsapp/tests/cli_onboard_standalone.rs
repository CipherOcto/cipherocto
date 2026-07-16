//! Smoke test: `octo-whatsapp onboard qr-link --help` must work WITHOUT a
//! running daemon. Onboarding is intentionally a daemon-free passthrough
//! (see `cli.rs::dispatch_onboard` / design §Onboarding passthrough), so
//! we must NOT spin up a daemon here — the binary alone, invoked on
//! `--help`, must exit 0 and emit clap's usage banner.

#[test]
fn cli_onboard_qr_link_help_works_without_daemon() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_octo-whatsapp"))
        .arg("onboard")
        .arg("qr-link")
        .arg("--help")
        .output()
        .expect("failed to spawn CLI");

    assert!(
        output.status.success(),
        "expected exit 0, got status: {:?}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    // clap's help banner always includes "Usage:" plus the subcommand name.
    assert!(
        stdout.contains("Usage:") || stdout.contains("qr-link"),
        "expected clap help banner mentioning 'Usage:' or 'qr-link', got: {stdout}"
    );
}
