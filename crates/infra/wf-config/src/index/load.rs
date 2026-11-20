use std::path::Path;

use wf_types::config::ConfigIndexFile;

use crate::error::{ConfigError, ConfigResult};

/// Load an index file. Accepts the file path or a directory (the directory's
/// `index.json` is used).
pub fn load_index_file(path: &Path) -> ConfigResult<ConfigIndexFile> {
    let file_path = if path.is_dir() {
        path.join(crate::layout::INDEX_FILE_NAME)
    } else {
        path.to_path_buf()
    };
    let index: ConfigIndexFile = crate::parser::parse_config_file(&file_path).map_err(|e| {
        crate::error::ConfigError::Parse(format!(
            "Failed to parse index file: {}: {e}",
            file_path.display()
        ))
    })?;
    if index.version != "1.0" {
        return Err(crate::error::ConfigError::Parse(format!(
            "Unsupported index version in {}: expected 1.0, got {}",
            file_path.display(),
            index.version
        )));
    }
    Ok(index)
}

/// Check if an index file exists at the given path (or directory).
pub fn index_file_exists(path: &Path) -> bool {
    let file_path = if path.is_dir() {
        path.join(crate::layout::INDEX_FILE_NAME)
    } else {
        path.to_path_buf()
    };
    file_path.exists()
}

/// Expand all glob patterns in the index file into absolute file paths
/// (deduplicated). Documentation samples (`*.example.*`) never match:
/// they describe schemas and must not leak into a running configuration.
pub fn expand_index_paths(index: &ConfigIndexFile, base_dir: &Path) -> ConfigResult<Vec<String>> {
    let mut result = Vec::new();
    for pattern in &index.paths {
        let full = base_dir.join(pattern);
        let full_str = full.to_string_lossy().to_string();
        if full_str.contains('*') {
            let entries = crate::loader::expand_glob_paths(&full_str).map_err(|e| {
                ConfigError::Parse(format!("invalid path pattern '{}' in index: {e}", pattern))
            })?;
            for entry in entries {
                if crate::layout::is_example_path(&entry) {
                    continue;
                }
                result.push(entry.to_string_lossy().to_string());
            }
        } else {
            result.push(full.to_string_lossy().to_string());
        }
    }
    result.sort();
    result.dedup();
    Ok(result)
}
