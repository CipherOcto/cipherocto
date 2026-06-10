fn main() {
    // SEC-C1: warn about TDLib supply chain risk when using download-tdlib
    #[cfg(feature = "download-tdlib")]
    #[cfg(not(any(feature = "local-tdlib", feature = "pkg-config")))]
    {
        println!("cargo::warning=SEC-C1: TDLib binary downloaded from GitHub without SHA verification.");
        println!("cargo::warning=SEC-C1: For production, use `pkg-config` with a system TDLib or");
        println!("cargo::warning=SEC-C1: vendor the prebuilt binary and verify its SHA256 manually.");
    }
}
