use clap::Parser;
use octo_whatsapp::cli::{dispatch, Cli};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    dispatch(cli)
}
