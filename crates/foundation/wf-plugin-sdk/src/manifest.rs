use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginType {
    Lua,
    Native,
}

/// Declared capability a plugin may touch. Declaration-only in this phase:
/// the engine treats permissions as audit/policy input (blocklist matching),
/// not as runtime capability enforcement.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PluginPermission {
    Filesystem,
    Network,
    Shell,
    Environment,
    Llm,
    Storage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginManifest {
    pub id: String,
    pub version: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub plugin_type: Option<PluginType>,
    pub sdk_version: Option<String>,
    pub entry_point: String,
    #[serde(default)]
    pub dependencies: HashMap<String, String>,
    #[serde(default)]
    pub optional_dependencies: HashMap<String, String>,
    #[serde(default)]
    pub contributions: Vec<String>,
    #[serde(default)]
    pub permissions: Vec<PluginPermission>,
    /// Optional JSON Schema (Draft 2020-12 subset: `type: object`, `required`,
    /// `properties.<k>.type`) describing the plugin's `config` payload. When
    /// present the host validates configs against it; absent means free-form.
    #[serde(default)]
    pub config_schema: Option<serde_json::Value>,
    #[serde(default)]
    pub config: Option<serde_json::Value>,
    #[serde(default)]
    pub hooks: Option<HashMap<String, String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_parses_permissions_snake_case() {
        let raw = r#"
id = "demo"
version = "1.0.0"
entry_point = "main.lua"
permissions = ["filesystem", "llm"]
"#;
        let manifest: PluginManifest = toml::from_str(raw).expect("parse manifest");
        assert_eq!(
            manifest.permissions,
            vec![PluginPermission::Filesystem, PluginPermission::Llm]
        );
    }

    #[test]
    fn manifest_rejects_unknown_permission() {
        let raw = r#"
id = "demo"
version = "1.0.0"
entry_point = "main.lua"
permissions = ["root_access"]
"#;
        assert!(toml::from_str::<PluginManifest>(raw).is_err());
    }

    #[test]
    fn manifest_rejects_unknown_fields() {
        let raw = r#"
id = "demo"
version = "1.0.0"
entry_point = "main.lua"
bogus = true
"#;
        assert!(toml::from_str::<PluginManifest>(raw).is_err());
    }
}
