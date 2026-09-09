//! wf-tui: full ratatui TUI over the wf-agent runtime.

// Re-export shared modules from wf-cli-shared.
#[cfg(feature = "embedded")]
pub use wf_cli_shared::default_runtime_config;
pub use wf_cli_shared::run as shared_run;
pub use wf_cli_shared::{
    app_config, approval_policy, args, cmd, config, domain, error, mode, output, remote, sanitize,
    turn, Cli, CliError, CliResult, Command, DiagWriter, HeadlessFileSink, OutputEnvelope,
    OutputFormat, OutputMessage, RunIo, RunOptions, RunOutcome, TeeSink,
};

// TUI-specific modules (ratatui-dependent).
pub mod animation;
pub mod ansi;
pub mod approval_overlay;
pub mod bottom_pane;
pub mod capabilities;
pub mod composer;
pub mod confirm_modal;
pub mod event_dispatch;
pub mod events;
pub mod fetch;
pub mod file_selection;
pub mod file_viewer;
pub mod footer;
pub mod framer;
pub mod help_modal;
pub mod history_cell;
pub mod interactive;
pub mod keymap;
pub mod markdown;
pub mod mention;
pub mod modal;
pub mod model_picker;
pub mod motion;
pub mod overlay;
pub mod panels;
pub mod password_modal;
pub mod probe;
pub mod question_overlay;
pub mod queue;
pub mod reducer;
pub mod render;
pub mod renderable;
pub mod replay;
pub mod screen_draw;
pub mod screens;
pub mod select;
pub mod sigint;
pub mod size;
pub mod state;
pub mod status_line;
pub mod stderr;
pub mod terminal;
pub mod theme;
pub mod transcript;
pub mod tui;
pub mod tui_debug;

use std::sync::Arc;

use wf_cli_shared::domain::DomainAdapter;
use wf_cli_shared::mode::{CliMode, ModeResolver};

/// CLI entry point: resolve the interactive form and dispatch.
pub async fn run(cli: Cli) -> CliResult<()> {
    if matches!(cli.command, Some(Command::DebugMode)) {
        return wf_cli_shared::debug_mode(&cli).await;
    }
    if matches!(cli.command, Some(Command::DebugTerminal { .. })) {
        return debug_terminal(&cli).await;
    }
    // Delegate subcommands and headless to shared library.
    match &cli.command {
        Some(Command::Workflow { .. })
        | Some(Command::Execution { .. })
        | Some(Command::LlmProfile { .. })
        | Some(Command::Skill { .. })
        | Some(Command::Search { .. })
        | Some(Command::Query { .. })
        | Some(Command::Checkpoint { .. })
        | Some(Command::Audit { .. })
        | Some(Command::Event { .. })
        | Some(Command::Variable { .. })
        | Some(Command::Message { .. })
        | Some(Command::Tool { .. })
        | Some(Command::Script { .. })
        | Some(Command::Trigger { .. })
        | Some(Command::Template { .. })
        | Some(Command::Approval { .. })
        | Some(Command::Task { .. })
        | Some(Command::Metrics { .. })
        | Some(Command::Analysis { .. })
        | Some(Command::Health)
        | Some(Command::Diagnostics) => {
            return wf_cli_shared::run(cli).await;
        }
        _ => {}
    }

    let (stdin_tty, stdout_tty) = mode::real_tty_status();
    let resolved = ModeResolver::resolve(&cli, stdin_tty, stdout_tty)?;

    match resolved.cli_mode {
        CliMode::Run => {
            // Delegate headless to shared library.
            wf_cli_shared::run(cli).await
        }
        CliMode::Tui => run_interactive(&cli, &resolved, stdout_tty).await,
    }
}

/// Interactive forms (full TUI).
async fn run_interactive(
    cli: &Cli,
    resolved: &wf_cli_shared::mode::ResolvedMode,
    stdout_tty: bool,
) -> CliResult<()> {
    if !stdout_tty {
        return Err(CliError::Arguments(format!(
            "interactive form {:?} requires a TTY (use `wf run` or --no-tui in pipes)",
            resolved.cli_mode
        )));
    }
    match resolved.cli_mode {
        CliMode::Tui => {
            let adapter = DomainAdapter::bootstrap_for_cli(cli, CliMode::Tui).await?;
            let app = crate::tui::TuiApp::new(Arc::new(adapter));
            app.run().await
        }
        CliMode::Run => unreachable!("run_interactive called with CliMode::Run"),
    }
}

/// Terminal facility probe (`wf debug-terminal`).
pub async fn debug_terminal(cli: &Cli) -> CliResult<()> {
    use crate::terminal::{install_panic_hook, CrosstermControl, TerminalGuard, TerminalModes};
    use crate::theme::{self, ThemeSource};

    let (_stdin_tty, stdout_tty) = mode::real_tty_status();
    let mut sink = wf_cli_shared::build_sink(cli, stdout_tty)?;
    let theme = theme::probe_theme();

    let Some(Command::DebugTerminal { alt_screen, exec }) = &cli.command else {
        return Err(CliError::Arguments(
            "debug-terminal dispatched wrongly".into(),
        ));
    };

    if !stdout_tty {
        sink.write_text(&format!(
            "[wf] no tty: terminal guard not activated (alt_screen={alt_screen}, would run {:?}); \
             theme {} kind {:?} ({}), domain {:?}",
            exec.clone()
                .or_else(|| std::env::var("EDITOR").ok())
                .unwrap_or_default(),
            theme.bg.hex(),
            theme.kind,
            match theme.source {
                ThemeSource::File => "file",
                ThemeSource::Probed => "probed",
                ThemeSource::Cached => "cached",
                ThemeSource::Default => "default fallback",
            },
            theme::ColorDomain::detect_from_env(),
        ))?;
        sink.flush()?;
        return Ok(());
    }

    install_panic_hook();
    let entered = if *alt_screen {
        TerminalModes::TUI
    } else {
        TerminalModes::MINI
    };
    let exec = exec
        .clone()
        .or_else(|| std::env::var("EDITOR").ok())
        .unwrap_or_else(|| "true".to_string());

    let mut guard = TerminalGuard::new(CrosstermControl::new(std::io::stdout()));
    guard.enter(entered)?;

    eprintln!("[frame] terminal modes active: {:?}", guard.modes());

    let exec_status = guard.with_restored(None, || {
        eprintln!("[with_restored] running: {exec}");
        std::process::Command::new("sh")
            .arg("-c")
            .arg(&exec)
            .status()
    })?;

    eprintln!(
        "[frame] redraw after with_restored (modes: {:?})",
        guard.modes()
    );

    let exit_ok = exec_status.map(|s| s.success()).unwrap_or(false);
    guard.restore()?;

    sink.write_text(&format!(
        "[wf] debug-terminal: modes entered {:?} / restored {:?}; exec {:?} -> {}; theme {} ({})",
        entered,
        guard.modes(),
        exec,
        if exit_ok { "ok" } else { "failed" },
        theme.bg.hex(),
        match theme.source {
            ThemeSource::File => "file",
            ThemeSource::Probed => "probed",
            ThemeSource::Cached => "cached",
            ThemeSource::Default => "default fallback",
        },
    ))?;
    sink.flush()?;
    Ok(())
}
