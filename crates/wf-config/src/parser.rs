use std::path::Path;

use crate::error::{ConfigError, ConfigResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFormat {
    Toml,
    Json,
}

pub fn config_format_from_path(path: &Path) -> ConfigResult<ConfigFormat> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("toml") => Ok(ConfigFormat::Toml),
        Some("json") => Ok(ConfigFormat::Json),
        other => Err(ConfigError::Parse(format!(
            "unsupported config format: {:?}",
            other
        ))),
    }
}

pub fn parse_toml<T: serde::de::DeserializeOwned>(content: &str) -> ConfigResult<T> {
    toml::from_str(content).map_err(|e| ConfigError::Parse(format!("TOML parse error: {e}")))
}

pub fn parse_json<T: serde::de::DeserializeOwned>(content: &str) -> ConfigResult<T> {
    serde_json::from_str(content).map_err(|e| ConfigError::Parse(format!("JSON parse error: {e}")))
}

pub fn parse_config<T: serde::de::DeserializeOwned>(
    content: &str,
    format: ConfigFormat,
) -> ConfigResult<T> {
    match format {
        ConfigFormat::Toml => parse_toml(content),
        ConfigFormat::Json => parse_json(content),
    }
}

pub fn parse_config_file<T: serde::de::DeserializeOwned>(path: &Path) -> ConfigResult<T> {
    let content = std::fs::read_to_string(path)?;
    let format = config_format_from_path(path)?;
    parse_config(&content, format)
}

#[cfg(test)]
#[allow(dead_code)]
mod tests {
    use super::*;

    #[test]
    fn test_config_format_from_path() {
        assert_eq!(
            config_format_from_path(Path::new("config.toml")).unwrap(),
            ConfigFormat::Toml
        );
        assert_eq!(
            config_format_from_path(Path::new("config.json")).unwrap(),
            ConfigFormat::Json
        );
        assert!(config_format_from_path(Path::new("config.yaml")).is_err());
        assert!(config_format_from_path(Path::new("config")).is_err());
    }

    #[test]
    fn test_parse_toml() {
        let toml_str = r#"
            name = "test"
            value = 42
        "#;
        #[derive(Debug, serde::Deserialize)]
        struct ParseTestConfig {
            #[allow(dead_code)]
            name: String,
            value: i32,
        }
        let result: ParseTestConfig = parse_toml(toml_str).unwrap();
        assert_eq!(result.name, "test");
        assert_eq!(result.value, 42);
    }

    #[test]
    fn test_parse_json() {
        let json_str = r#"{"name": "test", "value": 42}"#;
        #[derive(Debug, serde::Deserialize)]
        struct ParseTestConfig {
            #[allow(dead_code)]
            name: String,
            value: i32,
        }
        let result: ParseTestConfig = parse_json(json_str).unwrap();
        assert_eq!(result.name, "test");
        assert_eq!(result.value, 42);
    }

    #[test]
    fn test_parse_invalid_toml() {
        let bad = "this is not valid toml [[[";
        #[derive(Debug, serde::Deserialize)]
        struct ParseTestConfig {
            name: String,
        }
        assert!(parse_toml::<ParseTestConfig>(bad).is_err());
    }

    #[test]
    fn test_parse_invalid_json() {
        let bad = "not json";
        #[derive(Debug, serde::Deserialize)]
        struct ParseTestConfig {
            name: String,
        }
        assert!(parse_json::<ParseTestConfig>(bad).is_err());
    }
}
