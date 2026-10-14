//! TUI display config backup over `tui.toml`.
//!
//! Scope is the frontend display file only. Keymap settings and theme files
//! live beside it but are owned by their own loaders, so they stay out of
//! this command; copy those by hand when needed.

use crate::app_config::{config_file_path, ConfigManager};
use crate::args::{Cli, ConfigSub};
use crate::cmd::render::render_envelope;
use crate::error::{CliError, CliResult};
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &ConfigSub) -> CliResult<()> {
    let path = config_file_path()
        .ok_or_else(|| CliError::Configuration("config path unavailable".to_string()))?;
    match sub {
        ConfigSub::Export { dest } => {
            ConfigManager::export_to(&path, dest)
                .map_err(|e| CliError::Configuration(e.to_string()))?;
            render_envelope(
                cli.output,
                OutputEnvelope::success(
                    "config",
                    serde_json::json!({
                        "action": "export",
                        "source": path.to_string_lossy(),
                        "dest": dest.to_string_lossy(),
                    }),
                ),
            )?;
        }
        ConfigSub::Import { src } => {
            ConfigManager::import_from(src, &path)
                .map_err(|e| CliError::Configuration(e.to_string()))?;
            render_envelope(
                cli.output,
                OutputEnvelope::success(
                    "config",
                    serde_json::json!({
                        "action": "import",
                        "source": src.to_string_lossy(),
                        "dest": path.to_string_lossy(),
                    }),
                ),
            )?;
        }
    }
    Ok(())
}
