//! wf-headless binary entry point: parse arguments and hand off to the shared library.

use clap::Parser;

use wf_cli_shared::Cli;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if let Err(err) = wf_cli_shared::run_headless_only(cli).await {
        eprintln!("wf-headless: {err}");
        std::process::exit(i32::from(err.exit_code()));
    }
}
