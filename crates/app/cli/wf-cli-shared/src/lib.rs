//! wf-cli-shared: headless and shared logic for wf-cli targets.
//!
//! This crate contains the CLI argument parsing, output formatting, runtime
//! domain adapter, session runner, and all management subcommand handlers.
//! It has zero TUI/ratatui dependencies, enabling lightweight compilation
//! targets (headless, mini) without pulling in the full TUI dependency tree.

pub mod approval_policy;
pub mod app_config;
pub mod args;
pub mod cmd;
pub mod config;
pub mod domain;
pub mod error;
pub mod mode;
pub mod output;
pub mod remote;
pub mod run;
pub mod sanitize;
pub mod turn;

pub use args::{Cli, Command};
pub use error::{CliError, CliResult};
pub use output::{
    HeadlessFileSink, MemorySink, OutputEnvelope, OutputFormat, OutputMessage, TeeSink,
};
pub use run::{DiagWriter, RunIo, RunOptions, RunOutcome};

use crate::domain::DomainHandle;
use crate::mode::{CliMode, ModeResolver, ResolvedMode};
use crate::output::OutputSink;

/// Shared CLI entry point: resolve the interactive form and dispatch
/// subcommands. For headless-only binaries, use [`run_headless_only`].
pub async fn run(cli: Cli) -> CliResult<()> {
    match &cli.command {
        Some(Command::Workflow { sub }) => {
            return cmd::workflow::run(&cli, sub).await;
        }
        Some(Command::Execution { sub }) => {
            return cmd::execution::run(&cli, sub).await;
        }
        Some(Command::LlmProfile { sub }) => {
            return cmd::llm::run(&cli, sub).await;
        }
        Some(Command::Skill { sub }) => {
            return cmd::skill::run(&cli, sub).await;
        }
        Some(Command::Search { query, limit }) => {
            return cmd::search::run(&cli, query, *limit).await;
        }
        Some(Command::Query {
            status,
            workflow_id,
            limit,
            sort,
            desc,
            offset,
            aggregate,
            export,
            filter,
        }) => {
            return cmd::query::run(
                &cli,
                cmd::query::QueryOptions {
                    status: status.as_deref(),
                    workflow_id: workflow_id.as_deref(),
                    limit: *limit,
                    sort: sort.as_deref(),
                    desc: *desc,
                    offset: *offset,
                    aggregate: aggregate.as_deref(),
                    export: export.as_deref(),
                    filter: filter.as_deref(),
                },
            )
            .await;
        }
        Some(Command::Checkpoint { sub }) => {
            return cmd::checkpoint::run(&cli, sub).await;
        }
        Some(Command::Audit { sub }) => {
            return cmd::audit::run(&cli, sub).await;
        }
        Some(Command::Event { sub }) => {
            return cmd::event::run(&cli, sub).await;
        }
        Some(Command::Variable { sub }) => {
            return cmd::variable::run(&cli, sub).await;
        }
        Some(Command::Message { sub }) => {
            return cmd::message::run(&cli, sub).await;
        }
        Some(Command::Tool { sub }) => {
            return cmd::tool::run(&cli, sub).await;
        }
        Some(Command::Script { sub }) => {
            return cmd::script::run(&cli, sub).await;
        }
        Some(Command::Trigger { sub }) => {
            return cmd::trigger::run(&cli, sub).await;
        }
        Some(Command::Template { sub }) => {
            return cmd::template::run(&cli, sub).await;
        }
        Some(Command::Approval { sub }) => {
            return cmd::approval::run(&cli, sub).await;
        }
        Some(Command::Task { sub }) => {
            return cmd::task::run(&cli, sub).await;
        }
        Some(Command::Metrics { sub }) => {
            return cmd::metrics::run(&cli, sub).await;
        }
        Some(Command::Analysis { sub }) => {
            return cmd::analysis::run(&cli, sub).await;
        }
        Some(Command::Health) => {
            return cmd::diagnostics::run_health(&cli).await;
        }
        Some(Command::Diagnostics) => {
            return cmd::diagnostics::run_diagnostics(&cli).await;
        }
        _ => {}
    }

    let (stdin_tty, stdout_tty) = mode::real_tty_status();
    let resolved = ModeResolver::resolve(&cli, stdin_tty, stdout_tty)?;

    match resolved.cli_mode {
        CliMode::Run => run_headless(&cli, &resolved, stdout_tty).await,
        CliMode::Tui => {
            // TUI mode is not available in headless-only builds.
            // The full TUI binary (wf-cli) handles this path.
            Err(CliError::Arguments(
                "TUI mode is not supported in this build; use the `wf` binary for full TUI"
                    .to_string(),
            ))
        }
    }
}

/// Headless-only entry point: routes subcommands and headless sessions.
/// Returns error if TUI mode is requested.
pub async fn run_headless_only(cli: Cli) -> CliResult<()> {
    if matches!(cli.command, Some(Command::DebugMode)) {
        return debug_mode(&cli).await;
    }
    run(cli).await
}

/// Build the primary output sink for a CLI invocation: stdout (or the file /
/// pipe target) optionally teed into the `--log` file.
pub fn build_sink(cli: &Cli, stdout_tty: bool) -> CliResult<Box<dyn OutputSink + Send>> {
    let color = !cli.no_color && stdout_tty;
    let main: Box<dyn OutputSink + Send> = Box::new(HeadlessFileSink::stdout(cli.output, color));
    if let Some(path) = &cli.log {
        let file = HeadlessFileSink::file(path, cli.output, false)?;
        Ok(Box::new(TeeSink::new(vec![main, Box::new(file)])))
    } else {
        Ok(main)
    }
}

/// Headless single-session form (`wf run` / piped stdin / `--no-tui`).
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
            remote: _,
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

    let domain = DomainHandle::from_cli(cli, CliMode::Run).await?;
    let io = RunIo {
        sink,
        diag: std::sync::Arc::new(std::sync::Mutex::new(DiagWriter::stderr(diag_color))),
        format,
    };

    let session = match &domain {
        DomainHandle::Embedded(adapter) => run::run_session(adapter, opts, io).await,
        DomainHandle::Remote(remote) => run::run_session_remote(remote.client(), opts, io).await,
    };
    domain.shutdown().await?;
    session.map(|_| ())
}

/// Diagnostics for the `debug-mode` subcommand.
pub async fn debug_mode(cli: &Cli) -> CliResult<()> {
    let (stdin_tty, stdout_tty) = mode::real_tty_status();
    let resolved = ModeResolver::resolve(cli, stdin_tty, stdout_tty)?;
    let mut sink = build_sink(cli, stdout_tty)?;

    let data = serde_json::json!({
        "mode": match resolved.cli_mode {
            CliMode::Run => "run",
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

/// Construct a runtime config with CLI defaults.
#[cfg(feature = "embedded")]
pub fn default_runtime_config() -> wf_runtime::bootstrap::RuntimeConfig {
    wf_runtime::bootstrap::RuntimeConfig::default()
}
