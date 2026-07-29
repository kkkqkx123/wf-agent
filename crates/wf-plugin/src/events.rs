pub const PLUGIN_DISCOVERED: &str = "plugin:discovered";
pub const PLUGIN_LOADING: &str = "plugin:loading";
pub const PLUGIN_LOADED: &str = "plugin:loaded";
pub const PLUGIN_ACTIVATING: &str = "plugin:activating";
pub const PLUGIN_ACTIVATED: &str = "plugin:activated";
pub const PLUGIN_DEACTIVATING: &str = "plugin:deactivating";
pub const PLUGIN_DEACTIVATED: &str = "plugin:deactivated";
pub const PLUGIN_ERROR: &str = "plugin:error";

#[derive(Debug, Clone)]
pub enum PluginEvent {
    Discovered { plugin_id: String },
    Loading { plugin_id: String },
    Loaded { plugin_id: String, version: String },
    Activating { plugin_id: String },
    Activated { plugin_id: String },
    Deactivating { plugin_id: String },
    Deactivated { plugin_id: String },
    Error { plugin_id: String, error: String },
}
