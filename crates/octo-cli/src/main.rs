//! `octo` binary entrypoint — RFC-0011 §Binary Surface.

use clap::Parser;
use octo_cli::{commands, Octo};

fn main() {
    // `--help` / `--version` are not failures: clap renders them itself and
    // the process exits 0.
    let cli = match Octo::try_parse() {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };
    if let Err(e) = commands::dispatch(&cli) {
        let force_json = std::env::var_os("OCTO_FORCE_JSON").is_some();
        e.render(force_json);
    }
}
