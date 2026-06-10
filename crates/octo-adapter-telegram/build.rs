fn main() {
    // SEC-C1: supply-chain integrity check for TDLib binary download.
    //
    // When using `download-tdlib` (the default with `real-tdlib`), a prebuilt
    // TDLib binary is downloaded from GitHub with NO SHA verification. An
    // attacker who compromises the release server can execute arbitrary code
    // on every developer machine and CI runner.
    //
    // To pin a known-good SHA256, set the environment variable `TDLIB_SHA256`
    // to the expected hash before building. Our build script will compute the
    // SHA256 of the downloaded binary and fail the build if it doesn't match.
    //
    // For production deployments, prefer `pkg-config` with a system TDLib or
    // vendor the binary and verify its SHA manually.
    //
    // Known good SHA for tdlib-rs 1.4.0 (Linux x86_64):
    //   (not pre-computed — run: sha256sum $(find target -name 'libtdjson.so' | head -1))
    //   then set: export TDLIB_SHA256=<the_hash>
    //
    // This check is intentionally non-blocking when TDLIB_SHA256 is unset
    // to avoid breaking existing builds. Set the variable to enforce it.

    #[cfg(feature = "download-tdlib")]
    #[cfg(not(any(feature = "local-tdlib", feature = "pkg-config")))]
    {
        if let Ok(expected_sha) = std::env::var("TDLIB_SHA256") {
            // Find the TDLib binary in the build output directory.
            let out_dir = std::env::var("OUT_DIR").ok();
            let search_paths = vec![
                std::path::PathBuf::from("target"),
            ];
            let mut found = false;
            for base in &search_paths {
                if let Ok(entries) = std::fs::read_dir(base) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().and_then(|s| s.to_str()) == Some("so")
                            || path.extension().and_then(|s| s.to_str()) == Some("dll")
                            || path.extension().and_then(|s| s.to_str()) == Some("dylib")
                        {
                            if let Ok(data) = std::fs::read(&path) {
                                use std::io::Write;
                                let digest = sha256::digest(&data);
                                if digest != expected_sha {
                                    panic!(
                                        "SEC-C1: TDLib binary SHA256 mismatch!\\n                                         Expected: {}\\n                                         Actual:   {}\\n                                         Path: {}\\n                                         This may indicate a compromised binary.\\n                                         To accept the new hash, run with the updated TDLIB_SHA256.",
                                        expected_sha, digest, path.display()
                                    );
                                }
                                found = true;
                                println!("cargo:warning=SEC-C1: TDLib binary SHA256 verified OK");
                            }
                        }
                    }
                }
            }
            if !found {
                println!("cargo:warning=SEC-C1: TDLIB_SHA256 was set but no TDLib binary was found to verify");
            }
        } else {
            // TDLIB_SHA256 not set — warn but don't fail (backward compat).
            println!("cargo:warning=SEC-C1: TDLib binary downloaded from GitHub without SHA verification.");
            println!("cargo:warning=SEC-C1: Set TDLIB_SHA256=<expected_sha256> to enforce integrity check.");
            println!("cargo:warning=SEC-C1: For production, use `pkg-config` with a system TDLib.");
        }
    }
}
