//! Machine-local declarative CLI plugins for the interactive shells.
//!
//! When the server is remote (or plugin-less), interface extensions that
//! only make sense on this machine still need a home. This loader scans
//! two plugin roots for `plugin.toml` manifests and collects their inline
//! declarative CLI contributions (`cli_commands`, `completion_sources`,
//! `setting_fields`, `tool_summaries`, `markdown_rules`):
//!
//! - the global root (`$XDG_CONFIG_HOME/wf/tui-plugins/`, user-owned, so
//!   trusted on sight);
//! - the project root (`./.wf/tui-plugins/` under the workspace the shell
//!   adopted), which loads only after an explicit per-directory approval.
//!
//! Nothing here executes plugin code: the manifest entry point is never
//! consulted, no script runtime starts, and every collected entry passes
//! the SDK `validate` check before it is returned. Shells tag the results
//! as local so they shadow same-id snapshot entries. Approval state lives
//! in `$XDG_CONFIG_HOME/wf/tui-plugin-approvals.json`, separate from any
//! server-side trust, because the local checkout and the server's business
//! directory may differ.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use wf_plugin_sdk::{
    PluginCliCommand, PluginCompletionSource, PluginManifest, PluginMarkdownRule,
    PluginSettingField, PluginToolSummary,
};

use crate::error::{CliError, CliResult};

/// What a local scan found: validated declarative contributions plus
/// human-readable notices (unparseable manifests, unapproved project
/// roots, invalid entries). Shells surface the notices once and keep
/// the contributions.
#[derive(Debug, Default)]
pub struct LocalCliScan {
    pub commands: Vec<PluginCliCommand>,
    pub completions: Vec<PluginCompletionSource>,
    pub settings: Vec<PluginSettingField>,
    pub tool_summaries: Vec<PluginToolSummary>,
    pub markdown_rules: Vec<PluginMarkdownRule>,
    pub notices: Vec<String>,
    /// Content fingerprint over the loaded manifests; shells compare it
    /// for equality the same way they compare snapshot revisions.
    pub fingerprint: u64,
}

/// Global plugin root following XDG conventions (`tui.toml` sibling).
pub fn default_global_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|p| !p.as_os_str().is_empty())
                .map(|home| home.join(".config"))
        })?;
    Some(base.join("wf").join("tui-plugins"))
}

/// Approval record path (`tui.toml` sibling).
pub fn default_approvals_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|p| !p.as_os_str().is_empty())
                .map(|home| home.join(".config"))
        })?;
    Some(base.join("wf").join("tui-plugin-approvals.json"))
}

/// Record approval for a project plugin root at its current fingerprint.
/// Separate helper (not called by the scan) so approval stays an explicit
/// user action rather than something a scan can grant itself.
pub fn approve_project_dir(
    approvals_path: &Path,
    project_dir: &Path,
    fingerprint: u64,
) -> CliResult<()> {
    let canonical = canonical_dir(project_dir);
    let mut record = read_approvals(approvals_path);
    record.insert(canonical, fingerprint);
    write_approvals(approvals_path, &record)
}

/// Project the validated local scan onto the snapshot wire shape with
/// the scan fingerprint as revision. Shells feed the result through the
/// same convert-and-merge path as the server snapshot, tagging every
/// entry as local so shadowing stays explicit.
pub fn local_snapshot(scan: &LocalCliScan) -> wf_types::cli::CliSnapshot {
    fn convert<T: serde::Serialize, U: serde::de::DeserializeOwned>(items: &[T]) -> Vec<U> {
        items
            .iter()
            .filter_map(|item| {
                serde_json::to_value(item)
                    .ok()
                    .and_then(|value| serde_json::from_value(value).ok())
            })
            .collect()
    }
    wf_types::cli::CliSnapshot {
        revision: scan.fingerprint,
        commands: convert(&scan.commands),
        completions: convert(&scan.completions),
        settings: convert(&scan.settings),
        tool_summaries: convert(&scan.tool_summaries),
        markdown_rules: convert(&scan.markdown_rules),
    }
}

/// Scan both roots. Missing roots are not an error: a machine without
/// local plugins behaves exactly like a plugin-less server.
pub fn scan_local_cli(
    global_dir: Option<&Path>,
    project_dir: Option<&Path>,
    approvals_path: Option<&Path>,
) -> LocalCliScan {
    let mut scan = LocalCliScan::default();
    let mut hasher = DefaultHasher::new();
    if let Some(dir) = global_dir {
        scan_dir(dir, &mut scan, &mut hasher);
    }
    if let (Some(dir), Some(approvals)) = (project_dir, approvals_path) {
        scan_project_dir(dir, approvals, &mut scan, &mut hasher);
    }
    scan.fingerprint = hasher.finish();
    scan
}

/// Scan one root directory for plugin manifests. Approval gating
/// happens in the caller ([`scan_project_dir`]); this only reads.
fn scan_dir(dir: &Path, scan: &mut LocalCliScan, hasher: &mut DefaultHasher) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    let mut plugin_dirs: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir())
        .collect();
    plugin_dirs.sort();
    for plugin_dir in plugin_dirs {
        let manifest_path = plugin_dir.join("plugin.toml");
        if !manifest_path.is_file() {
            continue;
        }
        match load_manifest(&manifest_path) {
            Ok((manifest, bytes)) => {
                manifest.id.hash(hasher);
                bytes.hash(hasher);
                collect_manifest(&manifest, &plugin_dir, scan);
            }
            Err(message) => scan.notices.push(format!(
                "ignoring local plugin '{}': {message}",
                plugin_dir.display()
            )),
        }
    }
}

/// Scan the project root behind its approval gate. An unknown or changed
/// fingerprint skips the whole root with one notice: unapproved bytes are
/// never read beyond the manifest hash.
fn scan_project_dir(
    dir: &Path,
    approvals_path: &Path,
    scan: &mut LocalCliScan,
    hasher: &mut DefaultHasher,
) {
    if !dir.is_dir() {
        return;
    }
    let fingerprint = fingerprint_dir(dir);
    let canonical = canonical_dir(dir);
    let approved = read_approvals(approvals_path).get(&canonical) == Some(&fingerprint);
    if !approved {
        scan.notices.push(format!(
            "project plugin directory '{}' is not approved; approve it to load its CLI contributions",
            dir.display()
        ));
        return;
    }
    scan_dir(dir, scan, hasher);
}

/// Hash every manifest under a root without loading contributions: the
/// approval fingerprint must change whenever any declaration changes.
fn fingerprint_dir(dir: &Path) -> u64 {
    let mut hasher = DefaultHasher::new();
    let mut manifests: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.filter_map(|entry| entry.ok()) {
            let manifest = entry.path().join("plugin.toml");
            if manifest.is_file() {
                manifests.push(manifest);
            }
        }
    }
    manifests.sort();
    for manifest in manifests {
        manifest.hash(&mut hasher);
        if let Ok(bytes) = std::fs::read(&manifest) {
            bytes.hash(&mut hasher);
        }
    }
    hasher.finish()
}

fn canonical_dir(dir: &Path) -> String {
    std::fs::canonicalize(dir)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| dir.to_string_lossy().into_owned())
}

fn load_manifest(path: &Path) -> Result<(PluginManifest, Vec<u8>), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read manifest: {e}"))?;
    let text = std::str::from_utf8(&bytes).map_err(|e| format!("manifest is not UTF-8: {e}"))?;
    toml::from_str::<PluginManifest>(text)
        .map(|manifest| (manifest, bytes))
        .map_err(|e| format!("manifest does not parse: {e}"))
}

/// Move one manifest's inline CLI declarations into the scan, validating
/// each entry and keeping the valid ones. The `cli_ui` permission gates
/// all five kinds exactly like the engine's activation path, so a
/// manifest behaves the same locally and on the server. Plugin code is
/// never consulted: the entry point may name a script the local shell
/// will not run.
fn collect_manifest(manifest: &PluginManifest, plugin_dir: &Path, scan: &mut LocalCliScan) {
    use wf_plugin_sdk::PluginPermission;
    let has_cli = !manifest.cli_commands.is_empty()
        || !manifest.completion_sources.is_empty()
        || !manifest.setting_fields.is_empty()
        || !manifest.tool_summaries.is_empty()
        || !manifest.markdown_rules.is_empty();
    if has_cli && !manifest.permissions.contains(&PluginPermission::CliUi) {
        scan.notices.push(format!(
            "ignoring CLI contributions in '{}': manifest lacks the cli_ui permission",
            plugin_dir.display()
        ));
        return;
    }
    for command in &manifest.cli_commands {
        match command.validate() {
            Ok(()) => scan.commands.push(command.clone()),
            Err(e) => scan.notices.push(format!(
                "ignoring invalid command in '{}': {e}",
                plugin_dir.display()
            )),
        }
    }
    for source in &manifest.completion_sources {
        match source.validate() {
            Ok(()) => scan.completions.push(source.clone()),
            Err(e) => scan.notices.push(format!(
                "ignoring invalid completion source in '{}': {e}",
                plugin_dir.display()
            )),
        }
    }
    for field in &manifest.setting_fields {
        match field.validate() {
            Ok(()) => scan.settings.push(field.clone()),
            Err(e) => scan.notices.push(format!(
                "ignoring invalid setting field in '{}': {e}",
                plugin_dir.display()
            )),
        }
    }
    for summary in &manifest.tool_summaries {
        match summary.validate() {
            Ok(()) => scan.tool_summaries.push(summary.clone()),
            Err(e) => scan.notices.push(format!(
                "ignoring invalid tool summary in '{}': {e}",
                plugin_dir.display()
            )),
        }
    }
    for rule in &manifest.markdown_rules {
        match rule.validate() {
            Ok(()) => scan.markdown_rules.push(rule.clone()),
            Err(e) => scan.notices.push(format!(
                "ignoring invalid markdown rule in '{}': {e}",
                plugin_dir.display()
            )),
        }
    }
}

fn read_approvals(path: &Path) -> HashMap<String, u64> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    if text.trim().is_empty() {
        return HashMap::new();
    }
    serde_json::from_str(&text).unwrap_or_default()
}

fn write_approvals(path: &Path, record: &HashMap<String, u64>) -> CliResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(record)
        .map_err(|e| CliError::Configuration(format!("approval record encode: {e}")))?;
    std::fs::write(path, text)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_plugin_sdk::{PluginCliCondition, PluginPermission};

    fn manifest_toml(extra: &str) -> String {
        format!(
            "id = \"local.review\"\nversion = \"1.0.0\"\nname = \"Local review\"\n\
             entry_point = \"\"\npermissions = [\"cli_ui\"]\n{extra}"
        )
    }

    fn command_toml(id: &str, slash: &str) -> String {
        format!(
            "[[cli_commands]]\nid = \"{id}\"\ntitle = \"T\"\ngroup = \"G\"\n\
             description = \"D\"\nslash_name = \"{slash}\"\n"
        )
    }

    fn write_plugin(root: &Path, name: &str, manifest: &str) {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).expect("fixture dir");
        std::fs::write(dir.join("plugin.toml"), manifest).expect("fixture manifest");
    }

    #[test]
    fn global_scan_collects_valid_commands_and_skips_invalid_ones() {
        let root = std::env::temp_dir().join(format!(
            "wf-local-cli-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let global = root.join("global");
        write_plugin(
            &global,
            "good",
            &manifest_toml(&command_toml("local.run", "localrun")),
        );
        write_plugin(
            &global,
            "bad",
            &manifest_toml(&command_toml("bad id", "badrun")),
        );
        std::fs::create_dir_all(global.join("empty")).expect("empty plugin dir");
        let scan = scan_local_cli(Some(&global), None, None);
        assert_eq!(scan.commands.len(), 1);
        assert_eq!(scan.commands[0].id, "local.run");
        assert_eq!(scan.notices.len(), 1, "invalid entry reported once");
        assert_ne!(scan.fingerprint, 0);
        std::fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn project_root_requires_approval_and_detects_changes() {
        let root = std::env::temp_dir().join(format!(
            "wf-local-cli-proj-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let project = root.join("proj");
        let approvals = root.join("approvals.json");
        write_plugin(
            &project,
            "p",
            &manifest_toml(&command_toml("proj.run", "projrun")),
        );
        let denied = scan_local_cli(None, Some(&project), Some(&approvals));
        assert!(denied.commands.is_empty(), "unapproved root stays empty");
        assert_eq!(denied.notices.len(), 1);
        approve_project_dir(&approvals, &project, fingerprint_dir(&project)).expect("approve");
        let allowed = scan_local_cli(None, Some(&project), Some(&approvals));
        assert_eq!(allowed.commands.len(), 1);
        std::fs::write(
            project.join("p").join("plugin.toml"),
            manifest_toml(&command_toml("proj.run", "other")),
        )
        .expect("rewrite manifest");
        let changed = scan_local_cli(None, Some(&project), Some(&approvals));
        assert!(
            changed.commands.is_empty(),
            "changed manifest invalidates the approval"
        );
        std::fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn manifest_without_cli_fields_scans_clean() {
        let root = std::env::temp_dir().join(format!(
            "wf-local-cli-bare-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let global = root.join("global");
        write_plugin(&global, "bare", &manifest_toml(""));
        let scan = scan_local_cli(Some(&global), None, None);
        assert!(scan.commands.is_empty());
        assert!(scan.notices.is_empty());
        std::fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn approve_helper_round_trips_through_json() {
        let root = std::env::temp_dir().join(format!(
            "wf-local-cli-approve-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let approvals = root.join("approvals.json");
        let dir = root.join("proj");
        std::fs::create_dir_all(&dir).expect("fixture dir");
        approve_project_dir(&approvals, &dir, 42).expect("approve writes");
        assert_eq!(read_approvals(&approvals).len(), 1);
        let _ = PluginCliCondition::default();
        let _ = PluginPermission::CliUi;
        std::fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn inline_cli_without_permission_is_skipped() {
        let root = std::env::temp_dir().join(format!(
            "wf-local-cli-noperm-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let global = root.join("global");
        let manifest = manifest_toml(&command_toml("local.run", "localrun"))
            .replace("permissions = [\"cli_ui\"]\n", "");
        write_plugin(&global, "noperm", &manifest);
        let scan = scan_local_cli(Some(&global), None, None);
        assert!(scan.commands.is_empty());
        assert_eq!(scan.notices.len(), 1);
        assert!(scan.notices[0].contains("cli_ui"));
        std::fs::remove_dir_all(&root).expect("cleanup");
    }
}
