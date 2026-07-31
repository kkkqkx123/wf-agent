use std::path::{Path, PathBuf};

use crate::error::{ConfigError, ConfigResult};
use crate::parser;

pub async fn load_config_file<T: serde::de::DeserializeOwned>(path: &Path) -> ConfigResult<T> {
    let content = tokio::fs::read_to_string(path).await?;
    let format = parser::config_format_from_path(path)?;
    parser::parse_config(&content, format)
}

pub async fn try_load_config_file<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    load_config_file(path).await.ok()
}

pub fn load_config_file_sync<T: serde::de::DeserializeOwned>(path: &Path) -> ConfigResult<T> {
    let content = std::fs::read_to_string(path)?;
    let format = parser::config_format_from_path(path)?;
    parser::parse_config(&content, format)
}

pub fn file_exists(path: &Path) -> bool {
    path.exists()
}

pub fn resolve_config_path(base_dir: &Path, file_name: &str) -> PathBuf {
    base_dir.join(file_name)
}

pub fn find_config_file(base_dir: &Path, file_names: &[&str]) -> Option<PathBuf> {
    for name in file_names {
        let path = base_dir.join(name);
        if path.exists() {
            return Some(path);
        }
    }
    None
}

pub async fn load_config_from_paths<T: serde::de::DeserializeOwned>(
    paths: &[PathBuf],
) -> ConfigResult<(T, PathBuf)> {
    for path in paths {
        if let Ok(config) = load_config_file::<T>(path).await {
            return Ok((config, path.clone()));
        }
    }
    Err(ConfigError::NotFound(format!(
        "no valid config file found in: {:?}",
        paths
    )))
}

pub fn expand_glob_paths(pattern: &str) -> ConfigResult<Vec<PathBuf>> {
    let entries = glob::glob(pattern)
        .map_err(|e| ConfigError::Parse(format!("invalid glob pattern '{pattern}': {e}")))?;

    let mut paths = Vec::new();
    for entry in entries {
        match entry {
            Ok(path) => paths.push(path),
            Err(e) => {
                tracing::warn!("glob entry error: {e}");
            }
        }
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_file_exists() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "test").unwrap();
        assert!(file_exists(file.path()));
        assert!(!file_exists(Path::new("/nonexistent/path")));
    }

    #[test]
    fn test_resolve_config_path() {
        let base = Path::new("/config");
        let resolved = resolve_config_path(base, "app.toml");
        assert_eq!(resolved, Path::new("/config/app.toml"));
    }

    #[test]
    fn test_find_config_file() {
        let dir = std::env::temp_dir();
        let test_file = dir.join("test_wf_config_finder.toml");
        std::fs::write(&test_file, "test").unwrap();

        let result = find_config_file(&dir, &["nonexistent.toml", "test_wf_config_finder.toml"]);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), test_file);

        let result = find_config_file(&dir, &["nonexistent.toml"]);
        assert!(result.is_none());

        std::fs::remove_file(&test_file).ok();
    }

    #[test]
    fn test_load_config_file_sync() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_wf_config_loader.toml");
        std::fs::write(&path, r#"name = "test""#).unwrap();

        #[derive(Debug, serde::Deserialize)]
        struct TestConfig {
            name: String,
        }

        let result: TestConfig = load_config_file_sync(&path).unwrap();
        assert_eq!(result.name, "test");

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_expand_glob_paths() {
        let dir = std::env::temp_dir();
        let pattern = dir.join("test_wf_glob_*.toml");
        let pattern_str = pattern.to_string_lossy().to_string();

        let result = expand_glob_paths(&pattern_str).unwrap();
        assert!(result.is_empty() || result.iter().all(|p| p.exists()));

        let _ = std::fs::remove_file(dir.join("test_wf_glob_1.toml"));
    }
}
