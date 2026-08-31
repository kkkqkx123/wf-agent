//! wf-cli: headless run, mini and full TUI forms over the wf-agent runtime.

pub mod ansi;
pub mod approval;
pub mod approval_policy;
pub mod args;
pub mod cmd;
pub mod composer;
pub mod config;
pub mod domain;
pub mod error;
pub mod footer;
pub mod framer;
pub mod keymap;
pub mod markdown;
pub mod mention;
pub mod mini;
pub mod mode;
pub mod output;
pub mod panels;
pub mod question;
pub mod queue;
pub mod reducer;
pub mod render;
pub mod replay;
pub mod run;
pub mod sanitize;
pub mod scrollback;
pub mod select;
pub mod sink;
pub mod size;
pub mod terminal;
pub mod theme;
pub mod turn;

pub use ansi::AnsiParser;
pub use args::{Cli, Command};
pub use composer::Composer;
pub use error::{CliError, CliResult};
pub use footer::{Footer, FooterRoute, FooterView};
pub use framer::{FrameRateLimiter, FrameRequester};
pub use keymap::{Key, KeyAction, Keymap, KeymapContext};
pub use mini::{MiniApp, MiniOptions};
pub use output::{
    HeadlessFileSink, MemorySink, OutputEnvelope, OutputFormat, OutputMessage, TeeSink,
};
pub use run::{DiagWriter, RunIo, RunOptions, RunOutcome};
pub use scrollback::{HistoryLine, LineState, LinesView, Role};
pub use select::{Group, GroupItem, NavigateDir, SelectList};
pub use sink::{MiniOutputEvent, MiniSink};
pub use size::{ResizeDebouncer, Size};

use std::sync::Arc;

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
    match &cli.command {
        Some(Command::Workflow { sub }) => {
            return cmd::workflow::run(&cli, sub).await;
        }
        Some(Command::Execution { sub }) => {
            return cmd::execution::run(&cli, sub).await;
        }
        Some(Command::LlmProfile { sub: _ }) => {
            return cmd::llm::run(&cli).await;
        }
        Some(Command::Skill { sub: _ }) => {
            return cmd::skill::run(&cli).await;
        }
        Some(Command::Search { query, limit }) => {
            return cmd::search::run(&cli, query, *limit).await;
        }
        Some(Command::Query {
            status,
            workflow_id,
            limit,
        }) => {
            return cmd::query::run(&cli, status.as_deref(), workflow_id.as_deref(), *limit).await;
        }
        _ => {}
    }

    let (stdin_tty, stdout_tty) = mode::real_tty_status();
    let resolved = ModeResolver::resolve(&cli, stdin_tty, stdout_tty)?;

    match resolved.cli_mode {
        CliMode::Run => run_headless(&cli, &resolved, stdout_tty).await,
        CliMode::Mini | CliMode::Tui => run_interactive(&cli, &resolved, stdout_tty).await,
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
/// Bootstrap the runtime, drive one streaming agent or workflow session
/// ([`run::run_session`]) with the headless approval degradation, then tear
/// the runtime down preserving the session outcome.
async fn run_headless(cli: &Cli, resolved: &ResolvedMode, stdout_tty: bool) -> CliResult<()> {
    use std::io::IsTerminal;

    let format = cli.output;
    let sink = build_sink(cli, stdout_tty)?;
    let diag_color = !cli.no_color && std::io::stderr().is_terminal();

    let (arg_prompt, agent, model, approve_prefixes, workflow, input) = match &cli.command {
        Some(Command::Run {
            prompt,
            agent,
            model,
            approve_prefixes,
            workflow,
            input,
        }) => (
            prompt.clone(),
            agent.clone(),
            model.clone(),
            approve_prefixes.clone(),
            workflow.clone(),
            input.clone(),
        ),
        _ => (None, None, None, Vec::new(), None, None),
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
        workflow,
        workflow_input: input,
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

/// Interactive forms (mini / full TUI). Mini dispatches to [`MiniApp`]; the
/// full TUI renderer is not wired yet.
async fn run_interactive(cli: &Cli, resolved: &ResolvedMode, stdout_tty: bool) -> CliResult<()> {
    let cli_mode = resolved.cli_mode;
    if !stdout_tty {
        return Err(CliError::Arguments(format!(
            "interactive form {:?} requires a TTY (use `wf run` or --no-tui in pipes)",
            cli_mode
        )));
    }
    match cli_mode {
        CliMode::Mini => {
            let opts = MiniOptions {
                agent: cli.agent.clone(),
                model: cli.model.clone(),
                initial_prompt: cli.prompt.clone(),
                session_id: resolved.resume_session.clone(),
                resume_latest: resolved.resume_latest,
                storage_spec: cli.storage.clone(),
                adapter: Arc::new(DomainAdapter::bootstrap_for_cli(cli, CliMode::Mini).await?),
            };
            MiniApp::new(opts)?.run().await
        }
        CliMode::Tui => Err(CliError::Configuration(
            "full TUI not yet implemented; use --mini".into(),
        )),
        CliMode::Run => unreachable!("run_interactive called with CliMode::Run"),
    }
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
/// Exercises the whole terminal facility surface against the *real* terminal
/// when stdout is a TTY: theme detection, guard enter/restore, a
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
        return Err(CliError::Arguments(
            "debug-terminal dispatched wrongly".into(),
        ));
    };

    if !stdout_tty {
        // CI / pipe degradation path: no guard, default/cached theme.
        sink.write_text(&format!(
            "[wf] no tty: terminal guard not activated (alt_screen={alt_screen}, would run {:?}); \
             theme {} kind {:?} ({}), domain {:?}",
            exec.clone()
                .or_else(|| std::env::var("EDITOR").ok())
                .unwrap_or_default(),
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
