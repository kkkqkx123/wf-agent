//! Shared recursive layered merge for TOML configuration.
//!
//! Provides a generic mechanism to load multiple TOML files in priority order
//! (low → high) and recursively merge them. Sub-table fields are merged at
//! field level so partial overrides work at any depth.

use std::path::Path;

use serde::de::DeserializeOwned;

use crate::error::{ConfigError, ConfigResult};

/// Recursively merge `overlay` into `base` at the `toml::Value` level.
///
/// Table fields present in `overlay` overwrite those in `base`; sub-tables
/// are merged recursively so partial overrides work at any depth.
pub fn merge_toml_values(base: &mut toml::Value, overlay: toml::Value) {
    match (base, overlay) {
        (toml::Value::Table(base_t), toml::Value::Table(overlay_t)) => {
            for (key, val) in overlay_t {
                if base_t.contains_key(&key) {
                    merge_toml_values(&mut base_t[&key], val);
                } else {
                    base_t.insert(key, val);
                }
            }
        }
        (base, overlay) => *base = overlay,
    }
}

/// Load and merge TOML files from `paths` in order (lowest priority first),
/// then deserialize the merged result into `T`.
///
/// Missing files are silently skipped. Returns an error if no files exist
/// or if deserialization fails.
pub fn load_layered_config_sync<T: DeserializeOwned>(paths: &[&Path]) -> ConfigResult<T> {
    let mut base: Option<toml::Value> = None;

    for path in paths {
        if !path.exists() {
            continue;
        }
        let content = std::fs::read_to_string(path).map_err(|e| {
            ConfigError::Io(std::io::Error::other(format!(
                "failed to read {}: {}",
                path.display(),
                e
            )))
        })?;
        let overlay: toml::Value = toml::from_str(&content).map_err(|e| {
            ConfigError::Parse(format!("failed to parse {}: {}", path.display(), e))
        })?;

        match &mut base {
            Some(b) => merge_toml_values(b, overlay),
            None => base = Some(overlay),
        }
    }

    let merged = base.ok_or_else(|| {
        ConfigError::NotFound(format!(
            "no config files found in: {:?}",
            paths
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
        ))
    })?;

    // Serialize merged toml::Value back to string, then deserialize into T.
    // This round-trip is necessary because toml::Value -> T requires
    // going through serialization when T is not directly from Value.
    let serialized = toml::to_string_pretty(&merged).map_err(|e| {
        ConfigError::Serialization(format!("failed to serialize merged config: {e}"))
    })?;
    toml::from_str(&serialized)
        .map_err(|e| ConfigError::Parse(format!("failed to deserialize merged config: {e}")))
}

/// Async version of [`load_layered_config_sync`].
pub async fn load_layered_config<T: DeserializeOwned>(paths: &[&Path]) -> ConfigResult<T> {
    let mut base: Option<toml::Value> = None;

    for path in paths {
        if !path.exists() {
            continue;
        }
        let content = tokio::fs::read_to_string(path).await.map_err(|e| {
            ConfigError::Io(std::io::Error::other(format!(
                "failed to read {}: {}",
                path.display(),
                e
            )))
        })?;
        let overlay: toml::Value = toml::from_str(&content).map_err(|e| {
            ConfigError::Parse(format!("failed to parse {}: {}", path.display(), e))
        })?;

        match &mut base {
            Some(b) => merge_toml_values(b, overlay),
            None => base = Some(overlay),
        }
    }

    let merged = base.ok_or_else(|| {
        ConfigError::NotFound(format!(
            "no config files found in: {:?}",
            paths
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
        ))
    })?;

    let serialized = toml::to_string_pretty(&merged).map_err(|e| {
        ConfigError::Serialization(format!("failed to serialize merged config: {e}"))
    })?;
    toml::from_str(&serialized)
        .map_err(|e| ConfigError::Parse(format!("failed to deserialize merged config: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::io::Write;

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestConfig {
        name: String,
        #[serde(default)]
        value: i64,
        #[serde(default)]
        enabled: bool,
        #[serde(default)]
        nested: Option<NestedConfig>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct NestedConfig {
        #[serde(default)]
        port: u16,
        #[serde(default)]
        host: String,
    }

    #[test]
    fn test_merge_toml_values_table_override() {
        let mut base = toml::Value::try_from(serde_json::json!({
            "name": "base",
            "value": 1,
            "enabled": false
        }))
        .unwrap();
        let overlay = toml::Value::try_from(serde_json::json!({
            "name": "overlay",
            "enabled": true
        }))
        .unwrap();

        merge_toml_values(&mut base, overlay);

        assert_eq!(base["name"], toml::Value::String("overlay".to_string()));
        assert_eq!(base["value"], toml::Value::Integer(1));
        assert_eq!(base["enabled"], toml::Value::Boolean(true));
    }

    #[test]
    fn test_merge_toml_values_nested() {
        let mut base = toml::Value::try_from(serde_json::json!({
            "nested": { "port": 8080, "host": "localhost" }
        }))
        .unwrap();
        let overlay = toml::Value::try_from(serde_json::json!({
            "nested": { "port": 9090 }
        }))
        .unwrap();

        merge_toml_values(&mut base, overlay);

        assert_eq!(base["nested"]["port"], toml::Value::Integer(9090));
        assert_eq!(
            base["nested"]["host"],
            toml::Value::String("localhost".to_string())
        );
    }

    #[test]
    fn test_load_layered_config_sync() {
        let dir = std::env::temp_dir().join(format!("wf-layered-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let base_path = dir.join("base.toml");
        let overlay_path = dir.join("overlay.toml");

        let mut f = std::fs::File::create(&base_path).unwrap();
        writeln!(f, "name = \"base\"\nvalue = 1\nenabled = false").unwrap();

        let mut f = std::fs::File::create(&overlay_path).unwrap();
        writeln!(f, "name = \"overlay\"\nenabled = true").unwrap();

        let config: TestConfig =
            load_layered_config_sync(&[base_path.as_path(), overlay_path.as_path()]).unwrap();

        assert_eq!(config.name, "overlay");
        assert_eq!(config.value, 1);
        assert!(config.enabled);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_load_layered_config_sync_missing_files_skipped() {
        let dir = std::env::temp_dir().join(format!("wf-layered-skip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let base_path = dir.join("base.toml");
        let mut f = std::fs::File::create(&base_path).unwrap();
        writeln!(f, "name = \"base\"").unwrap();

        let missing = dir.join("missing.toml");
        let config: TestConfig =
            load_layered_config_sync(&[missing.as_path(), base_path.as_path()]).unwrap();

        assert_eq!(config.name, "base");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_load_layered_config_sync_no_files_returns_error() {
        let dir = std::env::temp_dir().join(format!("wf-layered-empty-{}", std::process::id()));
        let missing = dir.join("does_not_exist.toml");

        let result = load_layered_config_sync::<TestConfig>(&[missing.as_path()]);
        assert!(result.is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_load_layered_config_sync_nested_override() {
        let dir = std::env::temp_dir().join(format!("wf-layered-nested-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let base_path = dir.join("base.toml");
        let mut f = std::fs::File::create(&base_path).unwrap();
        writeln!(
            f,
            "name = \"test\"\n[nested]\nport = 8080\nhost = \"localhost\""
        )
        .unwrap();

        let overlay_path = dir.join("overlay.toml");
        let mut f = std::fs::File::create(&overlay_path).unwrap();
        writeln!(f, "[nested]\nport = 9090").unwrap();

        let config: TestConfig =
            load_layered_config_sync(&[base_path.as_path(), overlay_path.as_path()]).unwrap();

        let nested = config.nested.unwrap();
        assert_eq!(nested.port, 9090);
        assert_eq!(nested.host, "localhost");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
