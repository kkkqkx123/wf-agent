use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

use crate::args::Cli;
use crate::error::{CliError, CliResult};
use crate::output::{OutputEnvelope, OutputFormat};

static LOG_FILE: OnceLock<PathBuf> = OnceLock::new();

pub fn set_log_file(path: Option<PathBuf>) {
    if let Some(path) = path {
        let _ = LOG_FILE.set(path);
    }
}

fn log_path() -> Option<PathBuf> {
    LOG_FILE.get().cloned()
}

fn append_log_line(line: &str) -> CliResult<()> {
    if let Some(path) = log_path() {
        use std::fs::OpenOptions;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|err| {
                CliError::Configuration(format!(
                    "cannot append to log file '{}': {err}",
                    path.display()
                ))
            })?;
        writeln!(file, "{line}").map_err(CliError::from)?;
    }
    Ok(())
}

fn append_log_chunk(chunk: &str) -> CliResult<()> {
    if let Some(path) = log_path() {
        use std::fs::OpenOptions;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|err| {
                CliError::Configuration(format!(
                    "cannot append to log file '{}': {err}",
                    path.display()
                ))
            })?;
        write!(file, "{chunk}").map_err(CliError::from)?;
    }
    Ok(())
}

fn envelope_line(format: OutputFormat, envelope: &OutputEnvelope) -> CliResult<Option<String>> {
    match format {
        OutputFormat::Text => Ok(envelope.render(format)),
        OutputFormat::Json => Ok(Some(serde_json::to_string_pretty(envelope)?)),
        OutputFormat::JsonLines => Ok(Some(serde_json::to_string(envelope)?)),
        OutputFormat::Silent => Ok(None),
    }
}

pub fn render_envelope(format: OutputFormat, envelope: OutputEnvelope) -> CliResult<()> {
    if let Some(line) = envelope_line(format, &envelope)? {
        let mut stdout = std::io::stdout();
        writeln!(stdout, "{line}")?;
    }
    Ok(())
}

pub fn render_envelope_to_cli(cli: &Cli, envelope: OutputEnvelope) -> CliResult<()> {
    if let Some(line) = envelope_line(cli.output, &envelope)? {
        let mut stdout = std::io::stdout();
        writeln!(stdout, "{line}")?;
        append_log_line(&line)?;
    }
    Ok(())
}

pub fn print_line_to_cli(cli: &Cli, line: &str) -> CliResult<()> {
    println!("{line}");
    if cli.log.is_some() {
        append_log_line(line)?;
    }
    Ok(())
}

pub fn print_chunk_to_cli(cli: &Cli, chunk: &str) -> CliResult<()> {
    print!("{chunk}");
    let _ = std::io::Write::flush(&mut std::io::stdout());
    if cli.log.is_some() {
        append_log_chunk(chunk)?;
    }
    Ok(())
}

/// Render a failure envelope, then fail the command with the same message.
///
/// Resource-missing business failures stay machine-readable on stdout
/// while the process still exits non-zero for scripts and CI.
/// Validation verdicts are data, not failures, and do not use this path.
pub fn render_failure(format: OutputFormat, kind: &str, message: String) -> CliResult<()> {
    render_envelope(format, OutputEnvelope::failure(kind, message.clone()))?;
    Err(CliError::Business(message))
}

pub fn render_failure_to_cli(cli: &Cli, kind: &str, message: String) -> CliResult<()> {
    render_envelope_to_cli(cli, OutputEnvelope::failure(kind, message.clone()))?;
    Err(CliError::Business(message))
}
