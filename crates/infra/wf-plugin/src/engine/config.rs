use std::path::PathBuf;

use serde_json::Value;

/// Default runtime plugin scan paths: the user's global plugin directory
/// followed by the project-level override directory. The repository-root
/// `plugins/` directory is intentionally excluded so development samples are
/// not mistaken for installed plugins.
pub fn default_plugin_scan_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(home) = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|value| !value.is_empty())
    {
        paths.push(PathBuf::from(home).join(".wf").join("plugins"));
    }
    paths.push(PathBuf::from("./.wf/plugins"));
    paths
}

use super::PluginEngine;
use crate::contributions::OverridePolicy;
use crate::error::{PluginError, PluginResult};
use crate::manifest::{PluginManifest, PluginPermission};
use crate::signing::TrustedKeys;
use wf_plugin_sdk::manifest::{LuaConfig, WasmConfig};

/// Limit resolution order, weakest to strongest: built-in defaults, engine
/// globals below, per-plugin manifest declaration. Wasm `call_timeout_ms`
/// additionally falls back to `guard_timeout_ms` when neither level sets it.
/// Numeric limits never fail loading; only the gates below (plus manifest
/// validity, permission blocklist, ABI handshake, and signatures) can refuse
/// a plugin.
pub struct PluginSystemConfig {
    /// Library default stays closed so embedding hosts opt in explicitly;
    /// the product runtime enables it by default for out-of-box use.
    pub enabled: bool,
    pub paths: Vec<PathBuf>,
    pub auto_activate: bool,
    pub guard_timeout_ms: u64,
    pub override_policy: OverridePolicy,
    pub allow_list: Vec<String>,
    pub block_list: Vec<String>,
    /// Per-backend load gates. A disabled backend refuses its plugins at
    /// load with a clear error; this is the explicit off-switch, so limit
    /// values are never overloaded with enable semantics.
    pub lua_enabled: bool,
    pub native_enabled: bool,
    pub wasm_enabled: bool,
    /// Plugins declaring any of these permissions are refused at load time.
    pub required_permissions_blocklist: Vec<PluginPermission>,
    pub config: std::collections::HashMap<String, Value>,
    /// Engine-wide lua limit defaults applied when a manifest sets no `lua`
    /// values. `None` keeps the built-in defaults.
    pub lua_defaults: Option<LuaConfig>,
    /// Engine-wide wasm limit defaults applied when a manifest sets no
    /// `wasm` values. `None` keeps the built-in defaults.
    pub wasm_defaults: Option<WasmConfig>,
    pub signing: TrustedKeys,
}

impl Default for PluginSystemConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            paths: default_plugin_scan_paths(),
            auto_activate: true,
            guard_timeout_ms: 10000,
            override_policy: OverridePolicy::Forbid,
            allow_list: vec![],
            block_list: vec![],
            lua_enabled: true,
            native_enabled: false,
            wasm_enabled: true,
            required_permissions_blocklist: vec![],
            config: std::collections::HashMap::new(),
            lua_defaults: None,
            wasm_defaults: None,
            signing: TrustedKeys::default(),
        }
    }
}

impl PluginEngine {
    pub(crate) fn is_allowed(&self, plugin_id: &str) -> bool {
        if !self.options.allow_list.is_empty() {
            return self.options.allow_list.contains(&plugin_id.to_owned());
        }
        if !self.options.block_list.is_empty() {
            return !self.options.block_list.contains(&plugin_id.to_owned());
        }
        true
    }

    /// Reject plugins declaring permissions refused by host policy.
    pub(crate) fn check_permissions(&self, manifest: &PluginManifest) -> PluginResult<()> {
        let blocked = manifest
            .permissions
            .iter()
            .any(|p| self.options.required_permissions_blocklist.contains(p));
        if blocked {
            return Err(PluginError::PermissionDenied {
                plugin_id: manifest.id.clone(),
                reason: format!(
                    "plugin declares permissions blocked by host policy: {:?}",
                    manifest.permissions
                ),
            });
        }
        Ok(())
    }

    /// Run the shared admission checks for every loading entry point:
    /// manifest validity and permission policy. ABI compatibility is decided
    /// by the backend handshake after the module loads. Allowlist filtering
    /// stays with the caller so bulk discovery can skip silently while
    /// explicit single loads fail loudly.
    pub(crate) fn check_admission(&self, manifest: &PluginManifest) -> PluginResult<()> {
        if let Some(errors) = validate_manifest(manifest) {
            tracing::warn!("plugin '{}' manifest invalid: {:?}", manifest.id, errors);
            return Err(PluginError::InvalidManifest(errors.join(", ")));
        }
        self.check_permissions(manifest)?;
        Ok(())
    }
}

/// Evaluation input for an executable local UI backend (restricted
/// script runtime). The default conclusion is to not build it:立项当且仅当
/// declarative contributions cannot express the local interaction need.
#[derive(Debug, Clone, Default)]
pub struct ExecutableUiRequest {
    /// True when the need truly cannot be expressed declaratively.
    pub declarative_insufficient: bool,
    /// The backend stays default-off when built.
    pub default_off: bool,
    /// Host permission blacklist covers the new surface.
    pub blacklist_covers: bool,
    /// `tui.toml` cooperates (explicit opt-in, scoped grants).
    pub tui_toml_cooperates: bool,
    /// Failure posture falls back to built-in behavior.
    pub fallback_to_builtin: bool,
}

/// Binary evaluation output: whether to build the executable backend and
/// why. Wasm and Lua still only walk declarative contributions when built;
/// executable UI stays on the native trusted channel behind explicit
/// dispatch, with the host holding focus and dismissal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutableUiDecision {
    pub build: bool,
    pub reason: String,
}

/// Evaluate whether to stand up the executable local backend. Returns a
/// build/no-build decision, never a preset implementation.
pub fn evaluate_executable_ui(request: &ExecutableUiRequest) -> ExecutableUiDecision {
    if !request.declarative_insufficient {
        return ExecutableUiDecision {
            build: false,
            reason: "declarative contributions suffice; executable backend stays unbuilt".to_string(),
        };
    }
    let mut missing = Vec::new();
    if !request.default_off {
        missing.push("default-off");
    }
    if !request.blacklist_covers {
        missing.push("permission-blacklist");
    }
    if !request.tui_toml_cooperates {
        missing.push("tui.toml-cooperation");
    }
    if !request.fallback_to_builtin {
        missing.push("builtin-fallback");
    }
    if missing.is_empty() {
        ExecutableUiDecision {
            build: true,
            reason: "declarative-insufficient with all four guards satisfied".to_string(),
        }
    } else {
        ExecutableUiDecision {
            build: false,
            reason: format!("missing guards: {}", missing.join(", ")),
        }
    }
}

pub(crate) fn validate_manifest(manifest: &PluginManifest) -> Option<Vec<String>> {
    let mut errors = Vec::new();
    if manifest.id.is_empty() {
        errors.push("id is required".into());
    }
    if manifest.version.is_empty() {
        errors.push("version is required".into());
    } else if !crate::manifest::is_valid_plugin_version(&manifest.version) {
        errors.push(format!(
            "version '{}' is not valid semver",
            manifest.version
        ));
    }
    if manifest.entry_point.is_empty() {
        errors.push("entry_point is required".into());
    } else if manifest.entry_point.starts_with('/') || manifest.entry_point.contains("..") {
        errors.push(format!(
            "entry_point '{}' must be a relative path without parent traversal",
            manifest.entry_point
        ));
    }
    for (version, entry) in &manifest.versioned_entry_points {
        if *version == 0 {
            errors.push("versioned_entry_points keys must be non-zero ABI major versions".into());
        }
        if entry.is_empty() {
            errors.push(format!(
                "versioned_entry_points[{version}] must not be empty; provide a relative artifact path"
            ));
        } else if entry.starts_with('/') || entry.contains("..") {
            errors.push(format!(
                "versioned_entry_points[{version}] '{entry}' must be a relative path without parent traversal"
            ));
        }
    }
    if errors.is_empty() {
        None
    } else {
        Some(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn manifest(version: &str) -> PluginManifest {
        PluginManifest {
            id: "demo".into(),
            version: version.into(),
            name: None,
            description: None,
            plugin_type: None,
            entry_point: "main.lua".into(),
            versioned_entry_points: Default::default(),
            dependencies: Default::default(),
            optional_dependencies: Default::default(),
            contributions: vec![],
            permissions: vec![],
            config_schema: None,
            config: None,
            hooks: None,
            llm_providers: vec![],
            tools: vec![],
            tool_descriptions: vec![],
            cli_commands: vec![],
            completion_sources: vec![],
            setting_fields: vec![],
            tool_summaries: vec![],
            markdown_rules: vec![],
            wasm: None,
            lua: None,
        }
    }

    fn engine() -> PluginEngine {
        let options = PluginSystemConfig::default();
        PluginEngine::new(
            Arc::new(crate::registry::PluginRegistry::new()),
            Arc::new(crate::contributions::ContributionManager::new()),
            None,
            options,
        )
    }

    #[test]
    fn version_must_be_valid_semver() {
        assert!(validate_manifest(&manifest("1.0.0")).is_none());
        let errors = validate_manifest(&manifest("not-semver")).expect("must fail");
        assert!(
            errors.iter().any(|e| e.contains("semver")),
            "got: {errors:?}"
        );
    }

    #[test]
    fn versioned_entry_points_are_validated() {
        let mut m = manifest("1.0.0");
        m.versioned_entry_points.insert(0, "plugin.wasm".into());
        assert!(validate_manifest(&m).is_some());
        let mut m = manifest("1.0.0");
        m.versioned_entry_points.insert(1, "../escape.wasm".into());
        assert!(validate_manifest(&m).is_some());
        let mut m = manifest("1.0.0");
        m.versioned_entry_points.insert(1, "plugin_v1.wasm".into());
        assert!(validate_manifest(&m).is_none());
    }

    #[test]
    fn executable_backend_defaults_to_not_building() {
        assert!(!evaluate_executable_ui(&ExecutableUiRequest::default()).build);
        assert!(
            !evaluate_executable_ui(&ExecutableUiRequest {
                declarative_insufficient: true,
                default_off: true,
                blacklist_covers: false,
                tui_toml_cooperates: true,
                fallback_to_builtin: true,
            })
            .build
        );
        assert!(
            evaluate_executable_ui(&ExecutableUiRequest {
                declarative_insufficient: true,
                default_off: true,
                blacklist_covers: true,
                tui_toml_cooperates: true,
                fallback_to_builtin: true,
            })
            .build
        );
    }

    #[test]
    fn admission_rejects_bad_version() {
        let engine = engine();
        assert!(engine.check_admission(&manifest("1.0.0")).is_ok());
        assert!(engine.check_admission(&manifest("bad")).is_err());
    }
}
