//! wf-headless binary entry point: parse arguments and hand off to the shared library.

use clap::{CommandFactory, FromArgMatches};

use wf_cli_shared::{exit_with_error, parse_cli_or_exit, Cli};

/// Parse CLI arguments with this binary's display name, so version
/// attribution matches the invoked frontend instead of the shared default.
/// Usage errors funnel through the shared sysexits contract.
fn parse_cli() -> Cli {
    let command = Cli::command().display_name("wf-headless");
    parse_cli_or_exit(command, "wf-headless", Cli::from_arg_matches)
}

#[tokio::main]
async fn main() {
    let cli = parse_cli();
    if let Err(err) = wf_cli_shared::run_headless_only(cli).await {
        exit_with_error("wf-headless", &err);
    }
}
