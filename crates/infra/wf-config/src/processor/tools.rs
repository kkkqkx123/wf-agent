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

// ── code-context ────────────────────────────────────────────────

/// Raw code-context service config as loaded from a config file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CodeContextConfigInput {
    pub enabled: Option<bool>,
    pub timeout_ms: Option<u64>,
    pub transport_mode: Option<String>,
    pub base_url: Option<String>,
    pub managed_binary: Option<String>,
    pub managed_port: Option<u16>,
    pub managed_startup_timeout_ms: Option<u64>,
    pub fold_min_tokens: Option<usize>,
    pub fold_max_tokens: Option<usize>,
    pub fold_max_items: Option<usize>,
    pub fold_max_retries: Option<u32>,
    pub default_project_id: Option<i64>,
    pub default_limit: Option<usize>,
    pub max_results: Option<usize>,
}

fn parse_transport_mode(raw: Option<&str>) -> ConfigResult<wf_integration::TransportMode> {
    use wf_integration::TransportMode;
    match raw.map(str::trim).map(str::to_lowercase).as_deref() {
        None | Some("") | Some("external") => Ok(TransportMode::External),
        Some("managed") => Ok(TransportMode::Managed),
        Some(other) => Err(ConfigError::Validation(format!(
            "code-context transportMode must be 'external' or 'managed', got '{other}'"
        ))),
    }
}

pub fn validate_code_context_config(input: &CodeContextConfigInput) -> ConfigResult<()> {
    use wf_integration::TransportMode;
    let enabled = input.enabled.unwrap_or(false);
    let mode = parse_transport_mode(input.transport_mode.as_deref())?;
    if enabled {
        match mode {
            TransportMode::External => {
                if input
                    .base_url
                    .as_deref()
                    .is_none_or(|url| url.trim().is_empty())
                {
                    return Err(ConfigError::Validation(
                        "code-context baseUrl is required when the service is enabled with external transport".into(),
                    ));
                }
            }
            TransportMode::Managed => {
                if input
                    .managed_binary
                    .as_deref()
                    .is_some_and(|binary| binary.trim().is_empty())
                {
                    return Err(ConfigError::Validation(
                        "code-context managedBinary must not be blank".into(),
                    ));
                }
            }
        }
    }
    if let Some(url) = input.base_url.as_deref() {
        let trimmed = url.trim();
        if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
            return Err(ConfigError::Validation(
                "code-context baseUrl must start with http:// or https://".into(),
            ));
        }
    }
    if let Some(timeout_ms) = input.timeout_ms {
        if timeout_ms < 1 {
            return Err(ConfigError::Validation(
                "code-context timeoutMs must be at least 1".into(),
            ));
        }
    }
    if input.fold_max_tokens.is_some_and(|v| v < 1) {
        return Err(ConfigError::Validation(
            "code-context foldMaxTokens must be at least 1".into(),
        ));
    }
    if input.fold_max_items.is_some_and(|v| v < 1) {
        return Err(ConfigError::Validation(
            "code-context foldMaxItems must be at least 1".into(),
        ));
    }
    if input.fold_max_retries.is_some_and(|v| v < 1) {
        return Err(ConfigError::Validation(
            "code-context foldMaxRetries must be at least 1".into(),
        ));
    }
    if input.default_limit.is_some_and(|v| v < 1) {
        return Err(ConfigError::Validation(
            "code-context defaultLimit must be at least 1".into(),
        ));
    }
    if input.max_results.is_some_and(|v| v < 1) {
        return Err(ConfigError::Validation(
            "code-context maxResults must be at least 1".into(),
        ));
    }
    Ok(())
}

pub fn transform_code_context_config(
    input: CodeContextConfigInput,
) -> ConfigResult<wf_integration::CodeContextConfig> {
    use wf_integration::CodeContextConfig as Validated;

    validate_code_context_config(&input)?;
    let defaults = Validated::default();
    Ok(Validated {
        enabled: input.enabled.unwrap_or(false),
        transport: wf_integration::ServiceTransport {
            timeout_ms: input.timeout_ms.unwrap_or(defaults.transport.timeout_ms),
            transport_mode: parse_transport_mode(input.transport_mode.as_deref())?,
            base_url: input.base_url.and_then(|url| {
                let trimmed = url.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_string())
            }),
            managed_binary: input
                .managed_binary
                .and_then(|binary| {
                    let trimmed = binary.trim();
                    (!trimmed.is_empty()).then(|| trimmed.to_string())
                })
                .unwrap_or(defaults.transport.managed_binary),
            managed_port: input
                .managed_port
                .unwrap_or(defaults.transport.managed_port),
            managed_startup_timeout_ms: input
                .managed_startup_timeout_ms
                .unwrap_or(defaults.transport.managed_startup_timeout_ms),
        },
        fold: wf_integration::FoldPolicy {
            min_tokens: input.fold_min_tokens.unwrap_or(defaults.fold.min_tokens),
            max_tokens: input.fold_max_tokens.unwrap_or(defaults.fold.max_tokens),
            max_items: input.fold_max_items.unwrap_or(defaults.fold.max_items),
            max_retries: input.fold_max_retries.unwrap_or(defaults.fold.max_retries),
        },
        retrieval: wf_integration::RetrievalPolicy {
            default_project_id: input.default_project_id,
            default_limit: input
                .default_limit
                .unwrap_or(defaults.retrieval.default_limit),
            max_results: input.max_results.unwrap_or(defaults.retrieval.max_results),
        },
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

    #[test]
    fn code_context_transform_applies_defaults_and_validates() {
        let config = transform_code_context_config(CodeContextConfigInput::default()).unwrap();
        assert!(!config.enabled);
        assert_eq!(config.transport.timeout_ms, 60_000);
        assert_eq!(config.fold.max_tokens, 2000);
        assert_eq!(config.fold.min_tokens, 800);
        assert!(!config.is_usable());

        let err = transform_code_context_config(CodeContextConfigInput {
            enabled: Some(true),
            ..Default::default()
        })
        .unwrap_err();
        assert!(matches!(err, ConfigError::Validation(_)));

        let err = transform_code_context_config(CodeContextConfigInput {
            enabled: Some(true),
            base_url: Some("ftp://host".into()),
            ..Default::default()
        })
        .unwrap_err();
        assert!(matches!(err, ConfigError::Validation(_)));

        let err = transform_code_context_config(CodeContextConfigInput {
            transport_mode: Some("sidecar".into()),
            ..Default::default()
        })
        .unwrap_err();
        assert!(matches!(err, ConfigError::Validation(_)));

        let config = transform_code_context_config(CodeContextConfigInput {
            enabled: Some(true),
            base_url: Some("http://localhost:9000".into()),
            ..Default::default()
        })
        .unwrap();
        assert!(config.is_usable());
        assert!(config.external_base_url().is_some());

        let config = transform_code_context_config(CodeContextConfigInput {
            enabled: Some(true),
            transport_mode: Some("managed".into()),
            ..Default::default()
        })
        .unwrap();
        assert!(config.is_usable());
        assert!(config.external_base_url().is_none());
        let resolved = config.with_managed_address("http://127.0.0.1:9123".into());
        assert_eq!(
            resolved.external_base_url().as_deref(),
            Some("http://127.0.0.1:9123")
        );
    }
}
