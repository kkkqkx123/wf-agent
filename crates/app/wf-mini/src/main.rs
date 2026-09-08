//! wf-mini binary entry point: lightweight TUI using crossterm only.

mod app;
mod event;
mod markdown_mini;
mod renderer;
mod scrollback;
mod text_edit;

use clap::Parser;

use wf_cli_shared::Cli;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Headless subcommands bypass the TUI entirely.
    if wf_cli_shared::args::is_headless_command(&cli) {
        if let Err(err) = wf_cli_shared::run_headless_only(cli).await {
            eprintln!("wf-mini: {err}");
            std::process::exit(i32::from(err.exit_code()));
        }
        return;
    }

    // Resolve mode: force headless for non-TTY stdout.
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
            // Delegate to shared headless path.
            if let Err(err) = wf_cli_shared::run(cli).await {
                eprintln!("wf-mini: {err}");
                std::process::exit(i32::from(err.exit_code()));
            }
        }
        wf_cli_shared::mode::CliMode::Tui => {
            // Launch the mini TUI.
            let app = match app::MiniApp::new(&cli, &resolved).await {
                Ok(a) => a,
                Err(err) => {
                    eprintln!("wf-mini: {err}");
                    std::process::exit(i32::from(err.exit_code()));
                }
            };
            if let Err(err) = app.run().await {
                eprintln!("wf-mini: {err}");
                std::process::exit(i32::from(err.exit_code()));
            }
        }
    }
}
