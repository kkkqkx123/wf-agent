//! Tool configuration processors: validation and transformation for the
//! built-in file tools (glob / list-files / read-file).
//!
//! The Rust form uses `ConfigResult` instead of a `{valid, errors}` pair,
//! and the identity `exportXxx` helpers are omitted.

use serde::{Deserialize, Serialize};

use crate::error::{ConfigError, ConfigResult};

// ── glob ──────────────────────────────────────────────────────────

/// Raw glob tool config as loaded from a config file (all optional).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GlobConfigInput {
    pub workspace_dir: Option<String>,
    pub max_results: Option<u32>,
    pub enable_ignore: Option<bool>,
}

/// Validated glob tool config with defaults applied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlobConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_dir: Option<String>,
    pub max_results: u32,
    pub enable_ignore: bool,
}

const GLOB_MAX_RESULTS_DEFAULT: u32 = 50;

pub fn validate_glob_config(input: &GlobConfigInput) -> ConfigResult<()> {
    if let Some(max_results) = input.max_results {
        if max_results < 1 {
            return Err(ConfigError::Validation(
                "glob maxResults must be at least 1".into(),
            ));
        }
    }
    Ok(())
}

pub fn transform_glob_config(input: GlobConfigInput) -> ConfigResult<GlobConfig> {
    validate_glob_config(&input)?;
    Ok(GlobConfig {
        workspace_dir: input.workspace_dir,
        max_results: input.max_results.unwrap_or(GLOB_MAX_RESULTS_DEFAULT),
        enable_ignore: input.enable_ignore.unwrap_or(true),
    })
}

// ── list-files ────────────────────────────────────────────────────

/// Raw list_files tool config as loaded from a config file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListFilesConfigInput {
    pub workspace_dir: Option<String>,
    pub max_results: Option<u32>,
    pub enable_ignore: Option<bool>,
}

/// Validated list_files tool config with defaults applied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListFilesConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_dir: Option<String>,
    pub max_results: u32,
    pub enable_ignore: bool,
}

const LIST_FILES_MAX_RESULTS_DEFAULT: u32 = 1000;

pub fn validate_list_files_config(input: &ListFilesConfigInput) -> ConfigResult<()> {
    if let Some(max_results) = input.max_results {
        if max_results < 1 {
            return Err(ConfigError::Validation(
                "list-files maxResults must be at least 1".into(),
            ));
        }
    }
    Ok(())
}

pub fn transform_list_files_config(input: ListFilesConfigInput) -> ConfigResult<ListFilesConfig> {
    validate_list_files_config(&input)?;
    Ok(ListFilesConfig {
        workspace_dir: input.workspace_dir,
        max_results: input.max_results.unwrap_or(LIST_FILES_MAX_RESULTS_DEFAULT),
        enable_ignore: input.enable_ignore.unwrap_or(true),
    })
}

// ── read-file ─────────────────────────────────────────────────────

/// Raw read_file tool config as loaded from a config file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReadFileConfigInput {
    pub workspace_dir: Option<String>,
    pub max_file_size: Option<u64>,
    pub max_chars: Option<u64>,
    pub max_lines: Option<u64>,
    pub enable_ignore: Option<bool>,
    pub enable_protect: Option<bool>,
    pub model_id: Option<String>,
}

/// Validated read_file tool config with defaults applied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadFileConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_dir: Option<String>,
    pub max_file_size: u64,
    pub max_chars: u64,
    pub max_lines: u64,
    pub enable_ignore: bool,
    pub enable_protect: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
}

const READ_FILE_MAX_FILE_SIZE_DEFAULT: u64 = 500_000; // 500KB
const READ_FILE_MAX_CHARS_DEFAULT: u64 = 200_000; // 200K chars
const READ_FILE_MAX_LINES_DEFAULT: u64 = 2000;
const READ_FILE_ABSOLUTE_MAX_FILE_SIZE: u64 = 100 * 1024 * 1024; // 100MB

pub fn validate_read_file_config(input: &ReadFileConfigInput) -> ConfigResult<()> {
    if let Some(max_file_size) = input.max_file_size {
        if max_file_size > READ_FILE_ABSOLUTE_MAX_FILE_SIZE {
            return Err(ConfigError::Validation(
                "read-file maxFileSize exceeds the maximum allowed value (100MB)".into(),
            ));
        }
    }
    if let Some(max_chars) = input.max_chars {
        if max_chars > READ_FILE_ABSOLUTE_MAX_FILE_SIZE {
            return Err(ConfigError::Validation(
                "read-file maxChars exceeds the maximum allowed value".into(),
            ));
        }
    }
    if let Some(max_lines) = input.max_lines {
        if max_lines < 1 {
            return Err(ConfigError::Validation(
                "read-file maxLines must be at least 1".into(),
            ));
        }
    }
    Ok(())
}

pub fn transform_read_file_config(input: ReadFileConfigInput) -> ConfigResult<ReadFileConfig> {
    validate_read_file_config(&input)?;
    Ok(ReadFileConfig {
        workspace_dir: input.workspace_dir,
        max_file_size: input
            .max_file_size
            .unwrap_or(READ_FILE_MAX_FILE_SIZE_DEFAULT),
        max_chars: input.max_chars.unwrap_or(READ_FILE_MAX_CHARS_DEFAULT),
        max_lines: input.max_lines.unwrap_or(READ_FILE_MAX_LINES_DEFAULT),
        enable_ignore: input.enable_ignore.unwrap_or(false),
        enable_protect: input.enable_protect.unwrap_or(false),
        model_id: input.model_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_transform_applies_defaults() {
        let config = transform_glob_config(GlobConfigInput::default()).unwrap();
        assert_eq!(config.max_results, 50);
        assert!(config.enable_ignore);

        let err = transform_glob_config(GlobConfigInput {
            max_results: Some(0),
            ..Default::default()
        })
        .unwrap_err();
        assert!(matches!(err, ConfigError::Validation(_)));
    }

    #[test]
    fn list_files_transform_applies_defaults() {
        let config = transform_list_files_config(ListFilesConfigInput::default()).unwrap();
        assert_eq!(config.max_results, 1000);
        assert!(config.enable_ignore);
    }

    #[test]
    fn read_file_validation_and_transform() {
        let config = transform_read_file_config(ReadFileConfigInput::default()).unwrap();
        assert_eq!(config.max_file_size, 500_000);
        assert_eq!(config.max_lines, 2000);
        assert!(!config.enable_ignore);

        let err = transform_read_file_config(ReadFileConfigInput {
            max_file_size: Some(READ_FILE_ABSOLUTE_MAX_FILE_SIZE + 1),
            ..Default::default()
        })
        .unwrap_err();
        assert!(matches!(err, ConfigError::Validation(_)));

        let err = transform_read_file_config(ReadFileConfigInput {
            max_lines: Some(0),
            ..Default::default()
        })
        .unwrap_err();
        assert!(matches!(err, ConfigError::Validation(_)));
    }
}
