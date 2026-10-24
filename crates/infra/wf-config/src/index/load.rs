use super::*;

/// Load an index file. Accepts the file path or a directory (the directory's
/// `index.json` is used).
pub fn load_index_file(path: &Path) -> ConfigResult<ConfigIndexFile> {
    let file_path = if path.is_dir() {
        path.join(crate::preset::INDEX_FILE_NAME)
    } else {
        path.to_path_buf()
    };
    let index: ConfigIndexFile = crate::parser::parse_config_file(&file_path).map_err(|e| {
        ConfigError::Parse(format!(
            "Failed to parse index file: {}: {e}",
            file_path.display()
        ))
    })?;
    if index.version != "1.0" {
        return Err(ConfigError::Parse(format!(
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
        path.join(crate::preset::INDEX_FILE_NAME)
    } else {
        path.to_path_buf()
    };
    file_path.exists()
}

/// Expand all glob patterns in the index file into absolute file paths
/// (deduplicated).
pub fn expand_index_paths(index: &ConfigIndexFile, base_dir: &Path) -> Vec<String> {
    let mut result = Vec::new();
    for pattern in &index.paths {
        let full = base_dir.join(pattern);
        let full_str = full.to_string_lossy().to_string();
        if full_str.contains('*') {
            if let Ok(entries) = crate::loader::expand_glob_paths(&full_str) {
                for entry in entries {
                    result.push(entry.to_string_lossy().to_string());
                }
            }
        } else {
            result.push(full.to_string_lossy().to_string());
        }
    }
    result.sort();
    result.dedup();
    result
}

/// Build a [`ResolvedIndexEntry`] from extracted metadata parts.
pub(crate) fn build_entry(
    id: String,
    name: Option<String>,
    description: Option<String>,
    tags: Option<Vec<String>>,
    category: Option<String>,
    file_path: &str,
    extra: serde_json::Value,
) -> ResolvedIndexEntry {
    ResolvedIndexEntry {
        id,
        name,
        description,
        tags,
        category,
        file_path: file_path.to_string(),
        format: if file_path.to_lowercase().ends_with(".json") {
            ConfigFileFormat::Json
        } else {
            ConfigFileFormat::Toml
        },
        metadata: if extra.is_null() || extra.as_object().map(|o| o.is_empty()).unwrap_or(true) {
            None
        } else {
            Some(extra)
        },
    }
}

pub(crate) fn optional_string(value: &serde_json::Value, key: &str) -> Option<String> {
    value.get(key).and_then(|v| v.as_str()).map(str::to_string)
}

pub(crate) fn optional_tags(value: &serde_json::Value, key: &str) -> Option<Vec<String>> {
    value.get(key).and_then(|v| v.as_array()).map(|arr| {
        arr.iter()
            .filter_map(|t| t.as_str().map(str::to_string))
            .collect()
    })
}

// ── per-type config loaders (parse + validate + transform) ──────────────

pub(crate) fn load_llm_profile(path: &Path) -> ConfigResult<serde_json::Value> {
    let value = crate::parser::parse_config_file::<serde_json::Value>(path)?;
    let profile: wf_types::llm::LlmProfile = serde_json::from_value(value.clone())
        .map_err(|e| ConfigError::Parse(format!("invalid LLM profile: {e}")))?;
    crate::processor::llm_profile::validate_llm_profile(&profile)?;
    crate::processor::llm_profile::transform_llm_profile(
        &profile,
        &std::collections::HashMap::new(),
    )?;
    Ok(value)
}

pub(crate) fn load_llm_provider_definition(path: &Path) -> ConfigResult<serde_json::Value> {
    let value = crate::parser::parse_config_file::<serde_json::Value>(path)?;
    let definition: wf_types::llm::LlmProviderDefinition = serde_json::from_value(value.clone())
        .map_err(|e| ConfigError::Parse(format!("invalid LLM provider: {e}")))?;
    crate::processor::llm_profile::validate_provider_definition(&definition)?;
    Ok(value)
}

pub(crate) fn load_llm_auth(path: &Path) -> ConfigResult<serde_json::Value> {
    let value = crate::parser::parse_config_file::<serde_json::Value>(path)?;
    let provider: wf_types::config::llm_auth::AuthProviderConfig = serde_json::from_value(value.clone())
        .map_err(|e| ConfigError::Parse(format!("invalid LLM auth: {e}")))?;
    crate::processor::llm_auth::validate_auth_provider(&provider)?;
    Ok(value)
}

pub(crate) fn load_workflow(path: &Path) -> ConfigResult<serde_json::Value> {
    let value = crate::parser::parse_config_file::<serde_json::Value>(path)?;
    let workflow: wf_types::workflow::WorkflowDefinition = serde_json::from_value(value.clone())
        .map_err(|e| ConfigError::Parse(format!("invalid workflow: {e}")))?;
    crate::processor::workflow::validate_workflow_definition(&workflow)?;
    Ok(value)
}

pub(crate) fn load_node_template(path: &Path) -> ConfigResult<serde_json::Value> {
    let value = crate::parser::parse_config_file::<serde_json::Value>(path)?;
    let template: wf_types::workflow::node_template::NodeTemplate =
        serde_json::from_value(value.clone())
            .map_err(|e| ConfigError::Parse(format!("invalid node template: {e}")))?;
    crate::processor::node_template::validate_node_template(&template)?;
    crate::processor::node_template::transform_node_template(
        &template,
        &std::collections::HashMap::new(),
    )?;
    Ok(value)
}

pub(crate) fn load_script(path: &Path) -> ConfigResult<serde_json::Value> {
    let value = crate::parser::parse_config_file::<serde_json::Value>(path)?;
    let script: wf_script::ScriptDefinition = serde_json::from_value(value.clone())
        .map_err(|e| ConfigError::Parse(format!("invalid script: {e}")))?;
    crate::processor::script::validate_script_definition(&script)?;
    Ok(value)
}

pub(crate) fn load_prompt_template(path: &Path) -> ConfigResult<serde_json::Value> {
    let value = crate::parser::parse_config_file::<serde_json::Value>(path)?;
    let template: wf_types::template::Template = serde_json::from_value(value.clone())
        .map_err(|e| ConfigError::Parse(format!("invalid prompt template: {e}")))?;
    crate::processor::prompt::validate_prompt_template(&template)?;
    Ok(value)
}

pub(crate) fn load_agent_loop(path: &Path) -> ConfigResult<serde_json::Value> {
    let value = crate::parser::parse_config_file::<serde_json::Value>(path)?;
    let agent: wf_types::agent::AgentDefinition = serde_json::from_value(value.clone())
        .map_err(|e| ConfigError::Parse(format!("invalid agent loop: {e}")))?;
    crate::processor::agent_loop::validate_agent_definition(&agent)?;
    Ok(value)
}
