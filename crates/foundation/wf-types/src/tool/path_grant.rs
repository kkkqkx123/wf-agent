use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Operation a path grant relaxes at the approval layer.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PathGrantOperation {
    Read,
    Write,
    #[default]
    ReadWrite,
}

/// How an authorization is explained to the model. `None` still grants the
/// path at the approval layer but leaves the model unaware of the standing
/// permission.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PathDisclosure {
    /// List concrete paths in the incremental note.
    #[default]
    Paths,
    /// Announce only the preset category, not its paths.
    Category,
    /// Do not inject any model-facing note for this grant.
    None,
}

impl PathGrantOperation {
    pub fn allows_read(self) -> bool {
        matches!(self, Self::Read | Self::ReadWrite)
    }

    pub fn allows_write(self) -> bool {
        matches!(self, Self::Write | Self::ReadWrite)
    }
}

/// Declarative authorization preset selected before a session starts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PathGrantPreset {
    /// Absolute paths, `~/...` forms, or `{{TEMP_DIR}}` rooted paths.
    pub paths: Vec<String>,
    #[serde(default)]
    pub operation: PathGrantOperation,
    /// `false` marks paths outside checkpoint tracking: writes there cannot
    /// be restored. Such paths must be admitted through the execution scope
    /// as untracked prefixes, never by widening the workspace boundary.
    #[serde(default = "default_checkpointed")]
    pub checkpointed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_secs: Option<u64>,
    #[serde(default)]
    pub disclosure: PathDisclosure,
}

fn default_checkpointed() -> bool {
    true
}

impl Default for PathGrantPreset {
    fn default() -> Self {
        Self {
            paths: Vec::new(),
            operation: PathGrantOperation::ReadWrite,
            checkpointed: true,
            ttl_secs: None,
            disclosure: PathDisclosure::Paths,
        }
    }
}

/// One resolved authorization entry. Grants are evaluated by the approval
/// coordinator; they never bypass execution scope or checkpoint gates.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PathGrant {
    pub pattern: String,
    pub operation: PathGrantOperation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    #[serde(default = "default_checkpointed")]
    pub checkpointed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_secs: Option<u64>,
    #[serde(default)]
    pub disclosure: PathDisclosure,
}

/// Durable authorization state for one session or execution. It is the sole
/// source for approval grants and for model-facing authorization notes, so
/// restoring a checkpoint cannot produce a note that disagrees with the
/// permissions actually enforced.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AuthorizationState {
    #[serde(default)]
    pub applied_presets: Vec<String>,
    #[serde(default)]
    pub grants: Vec<PathGrant>,
    #[serde(default)]
    pub category_labels: Vec<String>,
}

impl AuthorizationState {
    pub fn from_grants(grants: Vec<PathGrant>) -> Self {
        let applied_presets = grants.iter().filter_map(|grant| grant.preset.clone()).fold(
            Vec::new(),
            |mut names, name| {
                if !names.contains(&name) {
                    names.push(name);
                }
                names
            },
        );
        Self {
            applied_presets,
            grants,
            category_labels: Vec::new(),
        }
    }

    /// Grants that may be shown to the model with concrete paths.
    pub fn disclosed_prefixes(&self) -> Vec<String> {
        let mut paths: Vec<String> = self
            .grants
            .iter()
            .filter(|grant| grant.disclosure == PathDisclosure::Paths)
            .map(|grant| grant.pattern.clone())
            .collect();
        paths.sort();
        paths.dedup();
        paths
    }

    /// Preset names that may be announced without exposing their paths.
    pub fn disclosed_categories(&self) -> Vec<String> {
        let mut labels: Vec<String> = self
            .grants
            .iter()
            .filter(|grant| grant.disclosure == PathDisclosure::Category)
            .filter_map(|grant| grant.preset.clone())
            .chain(self.category_labels.iter().cloned())
            .collect();
        labels.sort();
        labels.dedup();
        labels
    }

    /// Replace grants sourced from one preset without disturbing others.
    pub fn apply_preset(&mut self, name: &str, grants: Vec<PathGrant>) {
        self.grants
            .retain(|grant| grant.preset.as_deref() != Some(name));
        self.grants.extend(grants);
        if !self.applied_presets.iter().any(|item| item == name) {
            self.applied_presets.push(name.to_string());
        }
        self.applied_presets.sort();
    }

    pub fn revoke_preset(&mut self, name: &str) {
        self.grants
            .retain(|grant| grant.preset.as_deref() != Some(name));
        self.applied_presets.retain(|item| item != name);
    }
}

/// Name of the built-in preset rooted at the process temporary directory.
/// It avoids embedding any fixed temporary path in configuration or code.
pub const SCRATCH_PRESET: &str = "scratch";

fn expand_path(raw: &str) -> String {
    let normalized = raw.trim().replace('\\', "/");
    if let Some(rest) = normalized.strip_prefix("~/") {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| "~".to_string());
        return format!("{home}/{rest}");
    }
    if let Some(rest) = normalized.strip_prefix("{{TEMP_DIR}}/") {
        return format!("{}/{rest}", std::env::temp_dir().to_string_lossy());
    }
    if normalized == "{{TEMP_DIR}}" {
        return std::env::temp_dir().to_string_lossy().to_string();
    }
    normalized
}

/// Resolve active presets into concrete grants. User-defined presets win
/// over the built-in scratch preset of the same name, so a host can narrow
/// it without changing the runtime code.
pub fn expand_path_grants(
    presets: &BTreeMap<String, PathGrantPreset>,
    active: &[String],
) -> Vec<PathGrant> {
    let mut grants = Vec::new();
    for name in active {
        if let Some(preset) = presets.get(name) {
            for path in &preset.paths {
                grants.push(PathGrant {
                    pattern: expand_path(path),
                    operation: preset.operation,
                    preset: Some(name.clone()),
                    checkpointed: preset.checkpointed,
                    ttl_secs: preset.ttl_secs,
                    disclosure: preset.disclosure,
                });
            }
            continue;
        }
        if name == SCRATCH_PRESET {
            grants.push(PathGrant {
                pattern: expand_path("{{TEMP_DIR}}"),
                operation: PathGrantOperation::ReadWrite,
                preset: Some(name.clone()),
                checkpointed: false,
                ttl_secs: Some(3600),
                disclosure: PathDisclosure::Paths,
            });
        }
    }
    grants
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scratch_preset_uses_process_temp_dir() {
        let grants = expand_path_grants(&BTreeMap::new(), &[SCRATCH_PRESET.to_string()]);
        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0].pattern, std::env::temp_dir().to_string_lossy());
        assert!(!grants[0].checkpointed);
    }

    #[test]
    fn user_preset_replaces_builtin_scratch() {
        let mut presets = BTreeMap::new();
        presets.insert(
            SCRATCH_PRESET.to_string(),
            PathGrantPreset {
                paths: vec!["{{TEMP_DIR}}/wf".to_string()],
                operation: PathGrantOperation::Write,
                checkpointed: false,
                ttl_secs: Some(60),
                disclosure: PathDisclosure::Category,
            },
        );
        let grants = expand_path_grants(&presets, &[SCRATCH_PRESET.to_string()]);
        assert_eq!(grants.len(), 1);
        assert!(grants[0].pattern.ends_with("/wf"));
        assert!(grants[0].operation.allows_write());
        assert!(!grants[0].operation.allows_read());
    }
}

#[cfg(test)]
mod persistence_tests {
    use super::*;

    #[test]
    fn authorization_state_round_trips() {
        let state = AuthorizationState::from_grants(vec![PathGrant {
            pattern: "/home/user/.config".to_string(),
            operation: PathGrantOperation::ReadWrite,
            preset: Some("user_config".to_string()),
            checkpointed: false,
            ttl_secs: Some(60),
            disclosure: PathDisclosure::Category,
        }]);
        let json = serde_json::to_string(&state).expect("serialize authorization");
        let restored: AuthorizationState =
            serde_json::from_str(&json).expect("deserialize authorization");
        assert_eq!(state, restored);
        assert_eq!(
            restored.disclosed_categories(),
            vec!["user_config".to_string()]
        );
        assert!(restored.disclosed_prefixes().is_empty());
    }
}
