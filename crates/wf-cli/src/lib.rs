//! wf-cli: headless run, mini and full TUI forms over the wf-agent runtime.

pub mod args;
pub mod domain;
pub mod error;
pub mod events;
pub mod mode;
pub mod output;
pub mod run;
pub mod terminal;
pub mod theme;

pub use args::{Cli, Command};
pub use error::{CliError, CliResult};
pub use output::{HeadlessFileSink, MemorySink, OutputEnvelope, OutputFormat, OutputMessage, TeeSink};
pub use run::{DiagWriter, RunIo, RunOptions, RunOutcome};

use wf_runtime::bootstrap::RuntimeConfig;

use crate::domain::DomainAdapter;
use crate::mode::{CliMode, ModeResolver, ResolvedMode};
use crate::output::OutputSink;

/// CLI entry point: resolve the interactive form and dispatch.
pub async fn run(cli: Cli) -> CliResult<()> {
    if matches!(cli.command, Some(Command::DebugMode)) {
        return debug_mode(&cli).await;
    }
    if matches!(cli.command, Some(Command::DebugTerminal { .. })) {
        return debug_terminal(&cli).await;
    }

    let (stdin_tty, stdout_tty) = mode::real_tty_status();
    let resolved = ModeResolver::resolve(&cli, stdin_tty, stdout_tty)?;

    match resolved.cli_mode {
        CliMode::Run => run_headless(&cli, &resolved, stdout_tty).await,
        CliMode::Mini | CliMode::Tui => run_interactive(&cli, resolved.cli_mode, stdout_tty).await,
    }
}

/// Build the primary output sink for a CLI invocation: stdout (or the file /
/// pipe target) optionally teed into the `--log` file.
fn build_sink(cli: &Cli, stdout_tty: bool) -> CliResult<Box<dyn OutputSink + Send>> {
    let color = !cli.no_color && stdout_tty;
    let main: Box<dyn OutputSink + Send> = Box::new(HeadlessFileSink::stdout(cli.output, color));
    if let Some(path) = &cli.log {
        // Files never receive ANSI escapes.
        let file = HeadlessFileSink::file(path, cli.output, false)?;
        Ok(Box::new(TeeSink::new(vec![main, Box::new(file)])))
    } else {
        Ok(main)
    }
}

/// Headless single-session form (`wf run` / piped stdin / `--no-tui`).
///
/// Bootstrap the runtime, drive one streaming agent session
/// ([`run::run_session`]) with the headless approval degradation, then tear
/// the runtime down preserving the session outcome.
async fn run_headless(cli: &Cli, resolved: &ResolvedMode, stdout_tty: bool) -> CliResult<()> {
    use std::io::IsTerminal;

    let format = cli.output;
    let sink = build_sink(cli, stdout_tty)?;
    let diag_color = !cli.no_color && std::io::stderr().is_terminal();

    let (arg_prompt, agent, model, approve_prefixes) = match &cli.command {
        Some(Command::Run {
            prompt,
            agent,
            model,
            approve_prefixes,
        }) => (
            prompt.clone(),
            agent.clone(),
            model.clone(),
            approve_prefixes.clone(),
        ),
        _ => (None, None, None, Vec::new()),
    };
    let prompt = resolved
        .stdin_prompt
        .clone()
        .or(arg_prompt)
        .unwrap_or_default();

    let opts = RunOptions {
        prompt,
        agent_id: agent,
        model,
        approve_prefixes,
    };

    let adapter = DomainAdapter::bootstrap_for_cli(cli, CliMode::Run).await?;
    let io = RunIo {
        sink,
        diag: std::sync::Arc::new(std::sync::Mutex::new(DiagWriter::stderr(diag_color))),
        format,
    };

    // `run_session` owns the exit-code semantics (business failure → 1,
    // SIGINT → 4); shutdown must run even when the session fails.
    let session = run::run_session(&adapter, opts, io).await;
    adapter.shutdown().await?;
    session.map(|_| ())
}

/// Interactive forms (mini / full TUI). Stage 1 only reports the resolved
/// mode; the actual renderers land in Stage 6 (mini) and Stage 7 (TUI).
async fn run_interactive(cli: &Cli, cli_mode: CliMode, stdout_tty: bool) -> CliResult<()> {
    if !stdout_tty {
        return Err(CliError::Arguments(format!(
            "interactive form {:?} requires a TTY (use `wf run` or --no-tui in pipes)",
            cli_mode
        )));
    }
    let mut sink = build_sink(cli, stdout_tty)?;
    sink.write_text(&format!(
        "[wf] {:?} mode selected; the interactive renderer lands in a later stage",
        cli_mode
    ))?;
    sink.flush()?;
    Ok(())
}

/// Diagnostics for the `debug-mode` subcommand (resolved routing).
pub async fn debug_mode(cli: &Cli) -> CliResult<()> {
    let (stdin_tty, stdout_tty) = mode::real_tty_status();
    let resolved = ModeResolver::resolve(cli, stdin_tty, stdout_tty)?;
    let mut sink = build_sink(cli, stdout_tty)?;

    let data = serde_json::json!({
        "mode": match resolved.cli_mode {
            CliMode::Run => "run",
            CliMode::Mini => "mini",
            CliMode::Tui => "tui",
        },
        "outputFormat": format!("{:?}", cli.output),
        "stdinTty": stdin_tty,
        "stdoutTty": stdout_tty,
        "logFile": cli.log.as_ref().map(|p| p.to_string_lossy().to_string()),
    });
    let envelope = OutputEnvelope::success("debug", data);
    if let Some(line) = envelope.render(cli.output) {
        sink.write_raw(&line)?;
    }
    sink.flush()?;
    Ok(())
}

/// Construct a runtime config with CLI defaults (memory storage, warn
/// logging). Interactive forms and tests build on this.
pub fn default_runtime_config() -> RuntimeConfig {
    RuntimeConfig::default()
}

/// Terminal facility probe (`wf debug-terminal`).
///
/// Exercises the whole Stage 3 surface against the *real* terminal when
/// stdout is a TTY: theme detection, guard enter/restore, a
/// `with_restored` external-command window and the "redraw after re-enter"
/// duty. Without a TTY it only verifies the degradation paths (default
/// theme fallback, no guard activation) and exits 0.
pub async fn debug_terminal(cli: &Cli) -> CliResult<()> {
    use crate::terminal::{install_panic_hook, CrosstermControl, TerminalGuard, TerminalModes};
    use crate::theme::{self, ThemeSource};

    let (_stdin_tty, stdout_tty) = mode::real_tty_status();
    let mut sink = build_sink(cli, stdout_tty)?;
    let theme = theme::probe_theme();

    let Some(Command::DebugTerminal { alt_screen, exec }) = &cli.command else {
        return Err(CliError::Arguments("debug-terminal dispatched wrongly".into()));
    };

    if !stdout_tty {
        // CI / pipe degradation path: no guard, default/cached theme.
        sink.write_text(&format!(
            "[wf] no tty: terminal guard not activated (alt_screen={alt_screen}, would run {:?}); \
             theme {} kind {:?} ({}), domain {:?}",
            exec.clone().or_else(|| std::env::var("EDITOR").ok()).unwrap_or_default(),
            theme.bg.hex(),
            theme.kind,
            match theme.source {
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

    // Simulated frame while the modes are active (raw mode needs \r\n and
    // writes bypass the headless sink — stderr keeps stdout clean).
    eprintln!("[frame] terminal modes active: {:?}", guard.modes());

    let exec_status = guard.with_restored(None, || {
        eprintln!("[with_restored] running: {exec}");
        std::process::Command::new("sh")
            .arg("-c")
            .arg(&exec)
            .status()
    })?;

    // Redraw duty after the window: another simulated frame.
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
            ThemeSource::Probed => "probed",
            ThemeSource::Cached => "cached",
            ThemeSource::Default => "default fallback",
        },
    ))?;
    sink.flush()?;
    Ok(())
}
