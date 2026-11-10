//! wf-headless binary entry point: parse arguments and hand off to the shared library.

use clap::{CommandFactory, FromArgMatches};

use wf_cli_shared::Cli;

/// Parse CLI arguments with this binary's display name, so version
/// attribution matches the invoked frontend instead of the shared default.
fn parse_cli() -> Cli {
    let matches = Cli::command().display_name("wf-headless").get_matches();
    match Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(err) => err.exit(),
    }
}

#[tokio::main]
async fn main() {
    let cli = parse_cli();
    if let Err(err) = wf_cli_shared::run_headless_only(cli).await {
        eprintln!("wf-headless: {err}");
        std::process::exit(i32::from(err.exit_code()));
    }
}
