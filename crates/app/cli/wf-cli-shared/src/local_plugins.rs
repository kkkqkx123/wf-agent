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
//!
//! Fingerprints use `blake3` over manifest bytes, so approvals survive
//! toolchain updates. Each root is listed and read once per scan; the
//! project approval check and the contribution parse share those bytes.

use std::collections::HashMap;
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
pub struct LocalPluginScan {
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
    let Some(canonical) = canonical_dir(project_dir) else {
        return Err(CliError::Configuration(format!(
            "cannot approve '{}': directory does not resolve",
            project_dir.display()
        )));
    };
    let mut record = read_approvals(approvals_path);
    record.insert(canonical, fingerprint);
    write_approvals(approvals_path, &record)
}

/// Project the validated local scan onto the snapshot wire shape with
/// the scan fingerprint as revision. Shells feed the result through the
/// same convert-and-merge path as the server snapshot, tagging every
/// entry as local so shadowing stays explicit. Mapping is field-by-field
/// so an SDK/wire shape drift fails the build instead of dropping
/// entries at runtime.
pub fn local_snapshot(scan: &LocalPluginScan) -> wf_types::cli::CliSnapshot {
    wf_types::cli::CliSnapshot {
        revision: scan.fingerprint,
        commands: scan.commands.iter().map(map_command).collect(),
        completions: scan.completions.iter().map(map_completion_source).collect(),
        settings: scan.settings.iter().map(map_setting_field).collect(),
        tool_summaries: scan.tool_summaries.iter().map(map_tool_summary).collect(),
        markdown_rules: scan.markdown_rules.iter().map(map_markdown_rule).collect(),
    }
}

fn map_command(command: &PluginCliCommand) -> wf_types::cli::CliCommandView {
    wf_types::cli::CliCommandView {
        id: command.id.clone(),
        title: command.title.clone(),
        group: command.group.clone(),
        description: command.description.clone(),
        slash_name: command.slash_name.clone(),
        slash_aliases: command.slash_aliases.clone(),
        slash_argument: command.slash_argument.clone(),
        palette: command.palette,
        key_hint: command.key_hint.clone(),
        when: wf_types::cli::CliCommandCondition {
            screens: command.when.screens.clone(),
            roles: command.when.roles.clone(),
            require_connected: command.when.require_connected,
            hide_when_overlay: command.when.hide_when_overlay,
        },
    }
}

fn map_completion_source(source: &PluginCompletionSource) -> wf_types::cli::CliCompletionSource {
    wf_types::cli::CliCompletionSource {
        id: source.id.clone(),
        trigger_chars: source.trigger_chars.clone(),
        items: source
            .items
            .iter()
            .map(|item| wf_types::cli::CliCompletionItem {
                value: item.value.clone(),
                label: item.label.clone(),
                description: item.description.clone(),
            })
            .collect(),
    }
}

fn map_setting_field(field: &PluginSettingField) -> wf_types::cli::CliSettingField {
    wf_types::cli::CliSettingField {
        key: field.key.clone(),
        label: field.label.clone(),
        values: field.values.clone(),
        fallback: field.fallback.clone(),
    }
}

fn map_tool_summary(summary: &PluginToolSummary) -> wf_types::cli::CliToolSummary {
    wf_types::cli::CliToolSummary {
        tool_name: summary.tool_name.clone(),
        call_template: summary.call_template.clone(),
        result_template: summary.result_template.clone(),
    }
}

fn map_markdown_rule(rule: &PluginMarkdownRule) -> wf_types::cli::CliMarkdownRule {
    wf_types::cli::CliMarkdownRule {
        id: rule.id.clone(),
        pattern: rule.pattern.clone(),
        replacement: rule.replacement.clone(),
        surfaces: rule.surfaces.clone(),
    }
}

/// Scan both roots. Missing roots are not an error: a machine without
/// local plugins behaves exactly like a plugin-less server.
pub fn scan_local_plugins(
    global_dir: Option<&Path>,
    project_dir: Option<&Path>,
    approvals_path: Option<&Path>,
) -> LocalPluginScan {
    let mut scan = LocalPluginScan::default();
    let mut hasher = blake3::Hasher::new();
    if let Some(dir) = global_dir {
        scan_dir(dir, &mut scan, &mut hasher);
    }
    if let (Some(dir), Some(approvals)) = (project_dir, approvals_path) {
        scan_project_dir(dir, approvals, &mut scan, &mut hasher);
    }
    scan.fingerprint = stable_u64(hasher.finalize());
    scan
}

/// Scan one root directory for plugin manifests. Approval gating
/// happens in the caller ([`scan_project_dir`]); this only reads.
fn scan_dir(dir: &Path, scan: &mut LocalPluginScan, hasher: &mut blake3::Hasher) {
    for (manifest_path, result) in read_raw_entries(dir) {
        let plugin_dir = manifest_path.parent().map_or(dir, |parent| parent);
        let bytes = match result {
            Ok(bytes) => bytes,
            Err(message) => {
                scan.notices.push(format!(
                    "ignoring local plugin '{}': {message}",
                    plugin_dir.display()
                ));
                continue;
            }
        };
        update_hasher(hasher, plugin_dir, &bytes);
        match parse_manifest_bytes(&bytes) {
            Ok(manifest) => collect_manifest(&manifest, plugin_dir, scan),
            Err(message) => scan.notices.push(format!(
                "ignoring local plugin '{}': {message}",
                plugin_dir.display()
            )),
        }
    }
}

/// Scan the project root behind its approval gate. The directory is
/// listed and read once: the approval fingerprint and the contribution
/// parse share those bytes, so nothing can change between the check and
/// the load.
fn scan_project_dir(
    dir: &Path,
    approvals_path: &Path,
    scan: &mut LocalPluginScan,
    hasher: &mut blake3::Hasher,
) {
    if !dir.is_dir() {
        return;
    }
    let raw = read_raw_entries(dir);
    let mut approval_hasher = blake3::Hasher::new();
    for (manifest_path, result) in &raw {
        if let Ok(bytes) = result {
            let plugin_dir = manifest_path.parent().map_or(dir, |parent| parent);
            update_hasher(&mut approval_hasher, plugin_dir, bytes);
        }
    }
    let fingerprint = stable_u64(approval_hasher.finalize());
    let Some(canonical) = canonical_dir(dir) else {
        scan.notices.push(format!(
            "project plugin directory '{}' does not resolve; skipping its CLI contributions",
            dir.display()
        ));
        return;
    };
    let approved = read_approvals(approvals_path).get(&canonical) == Some(&fingerprint);
    if !approved {
        scan.notices.push(format!(
            "project plugin directory '{}' is not approved; run `wf plugin approve --dir '{}'` (record: '{}') to load its CLI contributions",
            dir.display(),
            dir.display(),
            approvals_path.display()
        ));
        return;
    }
    for (manifest_path, result) in raw {
        let plugin_dir = manifest_path
            .parent()
            .map_or_else(|| dir.to_path_buf(), |parent| parent.to_path_buf());
        let bytes = match result {
            Ok(bytes) => bytes,
            Err(message) => {
                scan.notices.push(format!(
                    "ignoring local plugin '{}': {message}",
                    plugin_dir.display()
                ));
                continue;
            }
        };
        update_hasher(hasher, &plugin_dir, &bytes);
        match parse_manifest_bytes(&bytes) {
            Ok(manifest) => collect_manifest(&manifest, &plugin_dir, scan),
            Err(message) => scan.notices.push(format!(
                "ignoring local plugin '{}': {message}",
                plugin_dir.display()
            )),
        }
    }
}

/// Hash every manifest under a root without loading contributions: the
/// approval fingerprint must change whenever any declaration changes.
pub fn fingerprint_dir(dir: &Path) -> u64 {
    let mut hasher = blake3::Hasher::new();
    for (manifest_path, result) in read_raw_entries(dir) {
        if let Ok(bytes) = result {
            let plugin_dir = manifest_path.parent().map_or(dir, |parent| parent);
            update_hasher(&mut hasher, plugin_dir, &bytes);
        }
    }
    stable_u64(hasher.finalize())
}

/// List `plugin.toml` manifests directly under one root, sorted for a
/// deterministic scan and fingerprint order.
fn manifest_paths(dir: &Path) -> Vec<PathBuf> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };
    let mut plugin_dirs: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir())
        .collect();
    plugin_dirs.sort();
    plugin_dirs
        .into_iter()
        .map(|plugin_dir| plugin_dir.join("plugin.toml"))
        .filter(|manifest| manifest.is_file())
        .collect()
}

/// Read every manifest under one root once. IO failures travel with the
/// path so the caller can report them without re-reading.
fn read_raw_entries(dir: &Path) -> Vec<(PathBuf, Result<Vec<u8>, String>)> {
    manifest_paths(dir)
        .into_iter()
        .map(|manifest| {
            let result = std::fs::read(&manifest).map_err(|e| format!("cannot read manifest: {e}"));
            (manifest, result)
        })
        .collect()
}

fn update_hasher(hasher: &mut blake3::Hasher, plugin_dir: &Path, bytes: &[u8]) {
    if let Some(name) = plugin_dir.file_name().and_then(|name| name.to_str()) {
        hasher.update(name.as_bytes());
    }
    hasher.update(&[0]);
    hasher.update(&(bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
    hasher.update(&[1]);
}

fn stable_u64(hash: blake3::Hash) -> u64 {
    let bytes = hash.as_bytes();
    let mut out = [0u8; 8];
    out.copy_from_slice(&bytes[..8]);
    u64::from_le_bytes(out)
}

fn canonical_dir(dir: &Path) -> Option<String> {
    std::fs::canonicalize(dir)
        .ok()
        .map(|path| path.to_string_lossy().into_owned())
}

fn parse_manifest_bytes(bytes: &[u8]) -> Result<PluginManifest, String> {
    let text = std::str::from_utf8(bytes).map_err(|e| format!("manifest is not UTF-8: {e}"))?;
    toml::from_str::<PluginManifest>(text).map_err(|e| format!("manifest does not parse: {e}"))
}

/// Move one manifest's inline CLI declarations into the scan, validating
/// each entry and keeping the valid ones. The `cli_ui` permission gates
/// all five kinds exactly like the engine's activation path, so a
/// manifest behaves the same locally and on the server. Plugin code is
/// never consulted: the entry point may name a script the local shell
/// will not run.
fn collect_manifest(manifest: &PluginManifest, plugin_dir: &Path, scan: &mut LocalPluginScan) {
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
    // A missing or corrupt record means nothing is approved: fail closed
    // and require a fresh explicit approval.
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
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(path, permissions)?;
    }
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
        let scan = scan_local_plugins(Some(&global), None, None);
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
        let denied = scan_local_plugins(None, Some(&project), Some(&approvals));
        assert!(denied.commands.is_empty(), "unapproved root stays empty");
        assert_eq!(denied.notices.len(), 1);
        approve_project_dir(&approvals, &project, fingerprint_dir(&project)).expect("approve");
        let allowed = scan_local_plugins(None, Some(&project), Some(&approvals));
        assert_eq!(allowed.commands.len(), 1);
        std::fs::write(
            project.join("p").join("plugin.toml"),
            manifest_toml(&command_toml("proj.run", "other")),
        )
        .expect("rewrite manifest");
        let changed = scan_local_plugins(None, Some(&project), Some(&approvals));
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
        let scan = scan_local_plugins(Some(&global), None, None);
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
        let scan = scan_local_plugins(Some(&global), None, None);
        assert!(scan.commands.is_empty());
        assert_eq!(scan.notices.len(), 1);
        assert!(scan.notices[0].contains("cli_ui"));
        std::fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn fingerprint_is_stable_for_identical_content() {
        let root = std::env::temp_dir().join(format!(
            "wf-local-cli-stable-{}",
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
        let first = fingerprint_dir(&global);
        let second = fingerprint_dir(&global);
        assert_eq!(first, second);
        assert_ne!(first, 0);
        std::fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn local_snapshot_maps_all_contribution_kinds_without_loss() {
        let root = std::env::temp_dir().join(format!(
            "wf-local-cli-snapshot-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let global = root.join("global");
        let manifest = manifest_toml(&format!(
            "{}\n[[completion_sources]]\nid = \"local.paths\"\ntrigger_chars = [\"@\"]\n\
             [[completion_sources.items]]\nvalue = \"src/\"\n\
             [[setting_fields]]\nkey = \"local.level\"\nlabel = \"Level\"\nfallback = \"low\"\n\
             [[tool_summaries]]\ntool_name = \"grep\"\ncall_template = \"search {{args}}\"\n\
             [[markdown_rules]]\nid = \"local.tickets\"\npattern = \"[T]\"\nreplacement = \"ticket\"\n",
            command_toml("local.run", "localrun")
        ));
        write_plugin(&global, "full", &manifest);
        let scan = scan_local_plugins(Some(&global), None, None);
        assert!(scan.notices.is_empty());
        let snapshot = local_snapshot(&scan);
        assert_eq!(snapshot.revision, scan.fingerprint);
        assert_eq!(snapshot.commands.len(), 1);
        assert_eq!(snapshot.completions.len(), 1);
        assert_eq!(snapshot.settings.len(), 1);
        assert_eq!(snapshot.tool_summaries.len(), 1);
        assert_eq!(snapshot.markdown_rules.len(), 1);
        assert_eq!(snapshot.commands[0].id, "local.run");
        assert_eq!(snapshot.commands[0].slash_name.as_deref(), Some("localrun"));
        assert_eq!(snapshot.completions[0].items.len(), 1);
        std::fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn approve_missing_directory_fails_loudly() {
        let root = std::env::temp_dir().join(format!(
            "wf-local-cli-missing-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let approvals = root.join("approvals.json");
        let missing = root.join("does-not-exist");
        assert!(approve_project_dir(&approvals, &missing, 1).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
