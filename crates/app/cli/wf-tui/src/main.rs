//! wf binary entry point: parse arguments and hand off to the library.

use clap::{CommandFactory, FromArgMatches};

use wf_cli_shared::{exit_with_error, parse_cli_or_exit, Cli};

#[tokio::main]
async fn main() {
    let command = Cli::command().display_name("wf");
    let cli: Cli = parse_cli_or_exit(command, "wf", Cli::from_arg_matches);
    if let Err(err) = wf_tui::run(cli).await {
        // Diagnostics go to stderr; stdout stays reserved for business output.
        exit_with_error("wf", &err);
    }
}
