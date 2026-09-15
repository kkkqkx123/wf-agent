pub const PLUGIN_DISCOVERED: &str = "plugin:discovered";
pub const PLUGIN_LOADING: &str = "plugin:loading";
pub const PLUGIN_LOADED: &str = "plugin:loaded";
pub const PLUGIN_ACTIVATING: &str = "plugin:activating";
pub const PLUGIN_ACTIVATED: &str = "plugin:activated";
pub const PLUGIN_DEACTIVATING: &str = "plugin:deactivating";
pub const PLUGIN_DEACTIVATED: &str = "plugin:deactivated";
pub const PLUGIN_ERROR: &str = "plugin:error";
pub const PLUGIN_CONFIG_CHANGED: &str = "plugin:config-changed";

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
        }
    }
}
