use pyo3_build_config::use_pyo3_cfgs;

fn main() {
    // Set linkage to static for musl
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "linux" {
        println!("cargo:rustc-link-libc=m");
    }

    use_pyo3_cfgs();
}
