pub const PLUGIN_DISCOVERED: &str = "plugin:discovered";
pub const PLUGIN_LOADING: &str = "plugin:loading";
pub const PLUGIN_LOADED: &str = "plugin:loaded";
pub const PLUGIN_ACTIVATING: &str = "plugin:activating";
pub const PLUGIN_ACTIVATED: &str = "plugin:activated";
pub const PLUGIN_DEACTIVATING: &str = "plugin:deactivating";
pub const PLUGIN_DEACTIVATED: &str = "plugin:deactivated";
pub const PLUGIN_ERROR: &str = "plugin:error";
pub const PLUGIN_CONFIG_CHANGED: &str = "plugin:config-changed";
pub const PLUGIN_RESOURCE_DISCOVERED: &str = "plugin:resource-discovered";

/// Incremental resource discovery kind: skill, prompt or theme path.
/// Theme discoveries are read-only consumption; the host never registers
/// a new theme from a plugin, it only reads the discovered paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    Skill,
    Prompt,
    Theme,
}

impl ResourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Skill => "skill",
            Self::Prompt => "prompt",
            Self::Theme => "theme",
        }
    }

    /// Whether the host may write through this kind. Theme stays
    /// read-only; skills and prompts merge into the host registries.
    pub fn is_writable(self) -> bool {
        !matches!(self, Self::Theme)
    }
}

impl std::str::FromStr for ResourceKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "skill" => Ok(Self::Skill),
            "prompt" => Ok(Self::Prompt),
            "theme" => Ok(Self::Theme),
            other => Err(format!("unrecognized resource kind '{other}'")),
        }
    }
}

/// Merge incremental discovery paths into the host set: appends unseen
/// entries in arrival order, drops exact duplicates. Auditable and
/// reproducible: the same increments always produce the same merged list.
pub fn merge_discovered_paths(current: &[String], incremental: &[String]) -> Vec<String> {
    let mut merged: Vec<String> = current.to_vec();
    for path in incremental {
        if path.trim().is_empty() {
            continue;
        }
        if !merged.contains(path) {
            merged.push(path.clone());
        }
    }
    merged
}

/// Gate project-level paths by trust: untrusted directories load zero
/// paths. Returns the kept paths; the caller surfaces the dropped count
/// with a hint so the approval action is discoverable.
pub fn gate_paths_by_trust(paths: &[String], is_trusted: bool) -> Vec<String> {
    if is_trusted {
        return paths.to_vec();
    }
    Vec::new()
}

#[derive(Debug, Clone)]
pub enum PluginEvent {
    Discovered {
        plugin_id: String,
    },
    Loading {
        plugin_id: String,
    },
    Loaded {
        plugin_id: String,
        version: String,
    },
    Activating {
        plugin_id: String,
    },
    Activated {
        plugin_id: String,
    },
    Deactivating {
        plugin_id: String,
    },
    Deactivated {
        plugin_id: String,
    },
    Error {
        plugin_id: String,
        error: String,
    },
    ConfigChanged {
        plugin_id: String,
        config: serde_json::Value,
    },
    ResourceDiscovered {
        plugin_id: String,
        kind: ResourceKind,
        paths: Vec<String>,
    },
}

impl PluginEvent {
    /// Lifecycle event type string (matches the `PLUGIN_*` constants).
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::Discovered { .. } => PLUGIN_DISCOVERED,
            Self::Loading { .. } => PLUGIN_LOADING,
            Self::Loaded { .. } => PLUGIN_LOADED,
            Self::Activating { .. } => PLUGIN_ACTIVATING,
            Self::Activated { .. } => PLUGIN_ACTIVATED,
            Self::Deactivating { .. } => PLUGIN_DEACTIVATING,
            Self::Deactivated { .. } => PLUGIN_DEACTIVATED,
            Self::Error { .. } => PLUGIN_ERROR,
            Self::ConfigChanged { .. } => PLUGIN_CONFIG_CHANGED,
            Self::ResourceDiscovered { .. } => PLUGIN_RESOURCE_DISCOVERED,
        }
    }

    /// Structured payload forwarded to plugin event handlers.
    pub fn payload(&self) -> serde_json::Value {
        match self {
            Self::Discovered { plugin_id }
            | Self::Loading { plugin_id }
            | Self::Activating { plugin_id }
            | Self::Activated { plugin_id }
            | Self::Deactivating { plugin_id }
            | Self::Deactivated { plugin_id } => serde_json::json!({ "plugin_id": plugin_id }),
            Self::Loaded { plugin_id, version } => {
                serde_json::json!({ "plugin_id": plugin_id, "version": version })
            }
            Self::Error { plugin_id, error } => {
                serde_json::json!({ "plugin_id": plugin_id, "error": error })
            }
            Self::ConfigChanged { plugin_id, config } => {
                serde_json::json!({ "plugin_id": plugin_id, "config": config })
            }
            Self::ResourceDiscovered { plugin_id, kind, paths } => {
                serde_json::json!({ "plugin_id": plugin_id, "kind": kind.as_str(), "paths": paths })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_merge_appends_incremental_paths_without_duplicates() {
        let merged = merge_discovered_paths(&["a".to_string()], &["b".to_string(), "a".to_string(), "".to_string()]);
        assert_eq!(merged, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn untrusted_directories_load_zero_paths() {
        let paths = vec!["skills/review".to_string()];
        assert!(gate_paths_by_trust(&paths, false).is_empty());
        assert_eq!(gate_paths_by_trust(&paths, true), paths);
    }

    #[test]
    fn theme_discoveries_stay_read_only() {
        assert!(!ResourceKind::Theme.is_writable());
        assert!(ResourceKind::Skill.is_writable());
        let event = PluginEvent::ResourceDiscovered {
            plugin_id: "review.bot".to_string(),
            kind: ResourceKind::Theme,
            paths: vec!["themes/dark.json".to_string()],
        };
        assert_eq!(event.event_type(), PLUGIN_RESOURCE_DISCOVERED);
        assert_eq!(event.payload()["kind"], "theme");
    }
}
