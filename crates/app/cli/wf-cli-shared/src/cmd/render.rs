use std::io::Write;

use crate::error::CliResult;
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
