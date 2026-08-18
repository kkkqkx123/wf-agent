//! wf-cli: headless run, mini and full TUI forms over the wf-agent runtime.
//!
//! Stage 1 delivers the output routing (format layer × sink layer) and the
//! domain adapter; the headless `run` subcommand closes the loop with a
//! bootstrap → output → shutdown wiring (streaming agent execution lands in
//! Stage 2).

pub mod args;
pub mod domain;
pub mod error;
pub mod events;
pub mod mode;
pub mod output;

pub use args::{Cli, Command};
pub use error::{CliError, CliResult};
pub use output::{HeadlessFileSink, MemorySink, OutputEnvelope, OutputFormat, OutputMessage, TeeSink};

use wf_runtime::bootstrap::RuntimeConfig;

use crate::domain::DomainAdapter;
use crate::mode::{CliMode, ModeResolver, ResolvedMode};
use crate::output::OutputSink;

/// CLI entry point: resolve the interactive form and dispatch.
pub async fn run(cli: Cli) -> CliResult<()> {
    if matches!(cli.command, Some(Command::DebugMode)) {
        return debug_mode(&cli).await;
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
/// Stage 1 wires the output contract: bootstrap the runtime, emit a readiness
/// record (text or envelope), then shut down. The streaming agent session
/// renderer replaces the placeholder in Stage 2.
async fn run_headless(cli: &Cli, resolved: &ResolvedMode, stdout_tty: bool) -> CliResult<()> {
    let mut sink = build_sink(cli, stdout_tty)?;

    let adapter = DomainAdapter::bootstrap_for_cli(cli, CliMode::Run).await?;

    let prompt = resolved
        .stdin_prompt
        .clone()
        .or_else(|| match &cli.command {
            Some(Command::Run { prompt, .. }) => prompt.clone(),
            _ => None,
        });

    if !cli.output.is_silent() {
        let data = serde_json::json!({
            "mode": "run",
            "outputFormat": format!("{:?}", cli.output),
            "promptChars": prompt.as_deref().map(str::len).unwrap_or(0),
        });
        let envelope = OutputEnvelope::success("execution", data);
        if let Some(line) = envelope.render(cli.output) {
            // `write_raw` bypasses the format filter so the envelope reaches
            // the sink in every format (json envelope on stdout, text line
            // otherwise).
            sink.write_raw(&line)?;
        }
        if cli.output == OutputFormat::Text {
            if let Some(p) = &prompt {
                sink.write_message(&OutputMessage::new("user", p))?;
            }
        }
    }
    sink.flush()?;

    adapter.shutdown().await?;
    Ok(())
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
