//! Machine-local TUI plugin approvals for project roots.
//!
//! Project plugin directories (`./.wf/tui-plugins/`) load only after an
//! explicit per-directory approval at the current content fingerprint.
//! This is the sole writer of the machine-local approval record
//! (`tui-plugin-approvals.json`); server-side trust stays separate, and a
//! changed manifest invalidates the approval so tampering fails closed.
//! Global plugins are trusted on sight and never appear here.

use std::path::PathBuf;

use crate::args::PluginSub;
use crate::error::{CliError, CliResult};
use crate::local_plugins;
use crate::output::{OutputEnvelope, OutputFormat};

fn resolve_dir(arg: Option<PathBuf>) -> CliResult<PathBuf> {
    if let Some(dir) = arg {
        return Ok(dir);
    }
    let current = std::env::current_dir().map_err(CliError::Io)?;
    Ok(current.join(".wf").join("tui-plugins"))
}

fn fingerprint_summary(dir: &std::path::Path) -> String {
    format!("{:016x}", local_plugins::fingerprint_dir(dir))
}

pub async fn run(cli: &crate::args::Cli, sub: &PluginSub) -> CliResult<()> {
    match sub {
        PluginSub::Status { dir } => {
            let dir = resolve_dir(dir.clone())?;
            let fingerprint = local_plugins::fingerprint_dir(&dir);
            let approvals_path = local_plugins::default_approvals_path().ok_or_else(|| {
                CliError::Configuration("cannot resolve the approval record path".to_string())
            })?;
            let canonical = std::fs::canonicalize(&dir)
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_else(|_| dir.to_string_lossy().into_owned());
            let approved = std::fs::read_to_string(&approvals_path)
                .ok()
                .and_then(|text| serde_json::from_str::<std::collections::HashMap<String, u64>>(&text).ok())
                .and_then(|record| record.get(&canonical).copied())
                == Some(fingerprint);
            let data = serde_json::json!({
                "dir": dir.to_string_lossy(),
                "fingerprint": fingerprint_summary(&dir),
                "fingerprint_raw": fingerprint,
                "approved": approved,
                "approvals_record": approvals_path.to_string_lossy(),
            });
            let envelope = OutputEnvelope::success("plugin-status", data);
            if let Some(line) = envelope.render(cli.output) {
                println!("{line}");
            }
            if cli.output == OutputFormat::Text {
                if approved {
                    println!("approved: {} ({})", dir.display(), fingerprint_summary(&dir));
                } else {
                    println!(
                        "not approved: {} ({}) — run `wf plugin approve --dir '{}'`",
                        dir.display(),
                        fingerprint_summary(&dir),
                        dir.display()
                    );
                }
            }
            Ok(())
        }
        PluginSub::Approve { dir, yes } => {
            let dir = resolve_dir(dir.clone())?;
            if !dir.is_dir() {
                return Err(CliError::Configuration(format!(
                    "project plugin directory '{}' does not exist",
                    dir.display()
                )));
            }
            let fingerprint = local_plugins::fingerprint_dir(&dir);
            let summary = fingerprint_summary(&dir);
            let approvals_path = local_plugins::default_approvals_path().ok_or_else(|| {
                CliError::Configuration("cannot resolve the approval record path".to_string())
            })?;
            if !yes {
                eprintln!("approve project plugins in '{}'?", dir.display());
                eprintln!("fingerprint: {summary}");
                eprintln!("record: {}", approvals_path.display());
                eprint!("confirm [y/N]: ");
                use std::io::Write;
                let _ = std::io::stderr().flush();
                let mut line = String::new();
                std::io::stdin()
                    .read_line(&mut line)
                    .map_err(CliError::Io)?;
                if !matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
                    return Err(CliError::Configuration("approval cancelled".to_string()));
                }
            }
            local_plugins::approve_project_dir(&approvals_path, &dir, fingerprint)?;
            let data = serde_json::json!({
                "dir": dir.to_string_lossy(),
                "fingerprint": summary,
                "fingerprint_raw": fingerprint,
                "approved": true,
                "approvals_record": approvals_path.to_string_lossy(),
            });
            let envelope = OutputEnvelope::success("plugin-approve", data);
            if let Some(line) = envelope.render(cli.output) {
                println!("{line}");
            }
            if cli.output == OutputFormat::Text {
                println!("approved {} ({summary}); rescan to load its CLI contributions", dir.display());
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approve_missing_directory_fails_loudly() {
        let missing = std::env::temp_dir().join("wf-plugin-approve-missing");
        let _ = std::fs::remove_dir_all(&missing);
        let approvals = std::env::temp_dir().join("wf-plugin-approve-record.json");
        assert!(local_plugins::approve_project_dir(&approvals, &missing, 1).is_err());
        let _ = std::fs::remove_file(&approvals);
    }

    #[test]
    fn fingerprint_summary_is_stable_hex() {
        let dir = std::env::temp_dir();
        assert_eq!(fingerprint_summary(&dir), fingerprint_summary(&dir));
        assert_eq!(fingerprint_summary(&dir).len(), 16);
    }
}
