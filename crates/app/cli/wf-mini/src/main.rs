//! wf-mini binary entry point: native terminal session. Prompts run in a
//! raw-mode single-row editor with ghost-text suggestions on TTYs and fall
//! back to the cooked line reader when piped.
//!
//! All session logic lives in the library target (`wf_mini`); this binary
//! only parses CLI arguments and dispatches.

use clap::{CommandFactory, FromArgMatches};

use wf_cli_shared::{exit_with_error, parse_cli_or_exit, Cli};

/// Parse CLI arguments with this binary's display name, so version
/// attribution matches the invoked frontend instead of the shared default.
/// Usage errors funnel through the shared sysexits contract.
fn parse_cli() -> Cli {
    let command = Cli::command().display_name("wf-mini");
    parse_cli_or_exit(command, "wf-mini", Cli::from_arg_matches)
}

#[tokio::main]
async fn main() {
    let cli = parse_cli();

    // Headless subcommands bypass the interactive session entirely.
    if wf_cli_shared::args::is_headless_command(&cli) {
        if let Err(err) = wf_cli_shared::run_headless_only(cli).await {
            exit_with_error("wf-mini", &err);
        }
        return;
    }

    // Resolve mode: non-TTY stdout falls back to the shared headless path.
    let (stdin_tty, stdout_tty) = wf_cli_shared::mode::real_tty_status();
    let resolved = match wf_cli_shared::mode::ModeResolver::resolve(&cli, stdin_tty, stdout_tty) {
        Ok(r) => r,
        Err(err) => exit_with_error("wf-mini", &err),
    };

    match resolved.cli_mode {
        wf_cli_shared::mode::CliMode::Run => {
            if let Err(err) = wf_cli_shared::run(cli).await {
                exit_with_error("wf-mini", &err);
            }
        }
        wf_cli_shared::mode::CliMode::Tui => match wf_mini::session::NativeSession::new(&cli).await
        {
            Ok(session) => {
                if let Err(err) = session.run().await {
                    exit_with_error("wf-mini", &err);
                }
            }
            Err(err) => exit_with_error("wf-mini", &err),
        },
    }
}
