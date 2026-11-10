use std::io::Write;

use crate::error::{CliError, CliResult};
use crate::output::{OutputEnvelope, OutputFormat};

pub fn render_envelope(format: OutputFormat, envelope: OutputEnvelope) -> CliResult<()> {
    match format {
        OutputFormat::Text => {
            let text = envelope.render(format);
            if let Some(line) = text {
                let mut stdout = std::io::stdout();
                writeln!(stdout, "{line}")?;
            }
            Ok(())
        }
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&envelope)?;
            let mut stdout = std::io::stdout();
            writeln!(stdout, "{json}")?;
            Ok(())
        }
        OutputFormat::JsonLines => {
            let json = serde_json::to_string(&envelope)?;
            let mut stdout = std::io::stdout();
            writeln!(stdout, "{json}")?;
            Ok(())
        }
        OutputFormat::Silent => Ok(()),
    }
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
