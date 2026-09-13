//! wf-mini binary entry point: native terminal session without raw mode.

mod approval;
mod input;
mod output;
mod session;
mod transcript;

use clap::Parser;

use wf_cli_shared::Cli;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Headless subcommands bypass the interactive session entirely.
    if wf_cli_shared::args::is_headless_command(&cli) {
        if let Err(err) = wf_cli_shared::run_headless_only(cli).await {
            eprintln!("wf-mini: {err}");
            std::process::exit(i32::from(err.exit_code()));
        }
        return;
    }

    // Resolve mode: non-TTY stdout falls back to the shared headless path.
    let (stdin_tty, stdout_tty) = wf_cli_shared::mode::real_tty_status();
    let resolved = match wf_cli_shared::mode::ModeResolver::resolve(&cli, stdin_tty, stdout_tty) {
        Ok(r) => r,
        Err(err) => {
            eprintln!("wf-mini: {err}");
            std::process::exit(i32::from(err.exit_code()));
        }
    };

    match resolved.cli_mode {
        wf_cli_shared::mode::CliMode::Run => {
            if let Err(err) = wf_cli_shared::run(cli).await {
                eprintln!("wf-mini: {err}");
                std::process::exit(i32::from(err.exit_code()));
            }
        }
        wf_cli_shared::mode::CliMode::Tui => match session::NativeSession::new(&cli).await {
            Ok(session) => {
                if let Err(err) = session.run().await {
                    eprintln!("wf-mini: {err}");
                    std::process::exit(i32::from(err.exit_code()));
                }
            }
            Err(err) => {
                eprintln!("wf-mini: {err}");
                std::process::exit(i32::from(err.exit_code()));
            }
        },
    }
}
