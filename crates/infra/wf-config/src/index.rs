//! Config index resolution and resolver registry.
//!
//! Index files contain only path patterns, metadata is extracted from the
//! individual config files, and per-type resolvers are registered into a
//! shared [`IndexRegistry`].
//!
//! Index files use the `{version, type, paths}` schema — the `type` field
//! is mapped onto [`IndexType`] via serde rename.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use wf_types::config::{
    ConfigFileFormat, ConfigIndexFile, IndexLoadFailure, IndexType, ResolvedIndex,
    ResolvedIndexEntry, ResolvedIndexMetadata,
};

use crate::error::{ConfigError, ConfigResult};

/// Resolver entry point: resolves an index file (or its directory) into a
/// [`ResolvedIndex`] with per-file metadata and collected failures.
pub type IndexResolver = Arc<
    dyn Fn(
            &Path,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = ConfigResult<ResolvedIndex>> + Send>,
        > + Send
        + Sync,
>;

/// A per-type config loader: parse + validate + transform a config file,
/// returning the raw value for metadata extraction. An `Err` is recorded as
/// an index failure.
pub type IndexConfigLoader = Arc<dyn Fn(&Path) -> ConfigResult<serde_json::Value> + Send + Sync>;

/// Metadata extractor: derive a [`ResolvedIndexEntry`] from a raw config value.
pub type IndexMetadataExtractor = fn(&serde_json::Value, &str) -> ResolvedIndexEntry;

pub struct IndexRegistry {
    resolvers: HashMap<IndexType, IndexResolver>,
}

impl IndexRegistry {
    pub fn new() -> Self {
        Self {
            resolvers: HashMap::new(),
        }
    }

    /// Register a resolver for an index type. Idempotent: re-registering an
    /// existing type replaces the previous resolver.
    pub fn register(&mut self, ty: IndexType, resolver: IndexResolver) -> ConfigResult<()> {
        self.resolvers.insert(ty, resolver);
        Ok(())
    }

    pub async fn resolve(&self, ty: &IndexType, path: &Path) -> ConfigResult<ResolvedIndex> {
        let resolver = self
            .resolvers
            .get(ty)
            .ok_or_else(|| ConfigError::Index(format!("no resolver registered for {:?}", ty)))?;
        resolver(path).await
    }

    pub fn has_resolver(&self, ty: &IndexType) -> bool {
        self.resolvers.contains_key(ty)
    }

    pub fn registered_types(&self) -> Vec<IndexType> {
        self.resolvers.keys().cloned().collect()
    }
}

impl Default for IndexRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Load an index file. Accepts the file path or a directory (the directory's
/// `index.json` is used).
pub fn load_index_file(path: &Path) -> ConfigResult<ConfigIndexFile> {
    let file_path = if path.is_dir() {
        path.join(crate::preset::INDEX_FILE_NAME)
    } else {
        path.to_path_buf()
    };
    crate::parser::parse_config_file(&file_path).map_err(|e| {
        ConfigError::Parse(format!(
            "Failed to parse index file: {}: {e}",
            file_path.display()
        ))
    })
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
fn build_entry(
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

fn optional_string(value: &serde_json::Value, key: &str) -> Option<String> {
    value.get(key).and_then(|v| v.as_str()).map(str::to_string)
}

fn optional_tags(value: &serde_json::Value, key: &str) -> Option<Vec<String>> {
    value.get(key).and_then(|v| v.as_array()).map(|arr| {
        arr.iter()
            .filter_map(|t| t.as_str().map(str::to_string))
            .collect()
    })
}

/// Extract metadata from an LLM Profile config (`id` / `name` /
/// `description` / `tags` / `provider` / `model`).
pub fn extract_llm_profile_metadata(
    value: &serde_json::Value,
    file_path: &str,
) -> ResolvedIndexEntry {
    let mut extra = serde_json::Map::new();
    if let Some(p) = optional_string(value, "provider") {
        extra.insert("provider".to_string(), serde_json::Value::String(p));
    }
    if let Some(m) = optional_string(value, "model") {
        extra.insert("model".to_string(), serde_json::Value::String(m));
    }
    build_entry(
        optional_string(value, "id").unwrap_or_default(),
        optional_string(value, "name"),
        optional_string(value, "description"),
        optional_tags(value, "tags"),
        None,
        file_path,
        serde_json::Value::Object(extra),
    )
}

/// Extract metadata from a Workflow config (`id` / `name` / `description` /
/// `tags` / `type` / `version` / `author`).
pub fn extract_workflow_metadata(value: &serde_json::Value, file_path: &str) -> ResolvedIndexEntry {
    let mut extra = serde_json::Map::new();
    if let Some(t) = optional_string(value, "type") {
        extra.insert("type".to_string(), serde_json::Value::String(t));
    }
    if let Some(v) = optional_string(value, "version") {
        extra.insert("version".to_string(), serde_json::Value::String(v));
    }
    if let Some(a) = value
        .get("metadata")
        .and_then(|m| m.get("author"))
        .and_then(|v| v.as_str())
    {
        extra.insert(
            "author".to_string(),
            serde_json::Value::String(a.to_string()),
        );
    }
    build_entry(
        optional_string(value, "id").unwrap_or_default(),
        optional_string(value, "name"),
        optional_string(value, "description"),
        optional_tags(value, "tags")
            .or_else(|| value.get("metadata").and_then(|m| optional_tags(m, "tags"))),
        None,
        file_path,
        serde_json::Value::Object(extra),
    )
}

/// Extract metadata from a Node Template config (id = `name`; `name` /
/// `description` / `node_type`).
pub fn extract_node_template_metadata(
    value: &serde_json::Value,
    file_path: &str,
) -> ResolvedIndexEntry {
    let name = optional_string(value, "name");
    let mut extra = serde_json::Map::new();
    if let Some(t) = optional_string(value, "node_type") {
        extra.insert("node_type".to_string(), serde_json::Value::String(t));
    }
    build_entry(
        name.clone().unwrap_or_default(),
        name,
        optional_string(value, "description"),
        optional_tags(value, "tags"),
        None,
        file_path,
        serde_json::Value::Object(extra),
    )
}

/// Extract metadata from a Script config (`id` = `id` or `name`; `name` /
/// `description` / `tags` / `category` / `executor`).
pub fn extract_script_metadata(value: &serde_json::Value, file_path: &str) -> ResolvedIndexEntry {
    let id = optional_string(value, "id")
        .or_else(|| optional_string(value, "name"))
        .unwrap_or_default();
    let mut extra = serde_json::Map::new();
    if let Some(e) =
        optional_string(value, "executor").or_else(|| optional_string(value, "executor_type"))
    {
        extra.insert("executor".to_string(), serde_json::Value::String(e));
    }
    build_entry(
        id,
        optional_string(value, "name"),
        optional_string(value, "description"),
        optional_tags(value, "tags"),
        optional_string(value, "category"),
        file_path,
        serde_json::Value::Object(extra),
    )
}

/// Extract metadata from a Prompt Template config (`id` / `name` /
/// `description` / `tags` / `category`).
pub fn extract_prompt_template_metadata(
    value: &serde_json::Value,
    file_path: &str,
) -> ResolvedIndexEntry {
    build_entry(
        optional_string(value, "id").unwrap_or_default(),
        optional_string(value, "name"),
        optional_string(value, "description"),
        optional_tags(value, "tags"),
        optional_string(value, "category"),
        file_path,
        serde_json::Value::Null,
    )
}

/// Extract metadata from an Agent Loop config (`id` / `name` /
/// `description` / `tags`).
pub fn extract_agent_loop_metadata(
    value: &serde_json::Value,
    file_path: &str,
) -> ResolvedIndexEntry {
    build_entry(
        optional_string(value, "id").unwrap_or_default(),
        optional_string(value, "name"),
        optional_string(value, "description"),
        optional_tags(value, "tags")
            .or_else(|| value.get("metadata").and_then(|m| optional_tags(m, "tags"))),
        None,
        file_path,
        serde_json::Value::Null,
    )
}

// ── per-type config loaders (parse + validate + transform) ──────────────

fn load_llm_profile(path: &Path) -> ConfigResult<serde_json::Value> {
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

fn load_workflow(path: &Path) -> ConfigResult<serde_json::Value> {
    let value = crate::parser::parse_config_file::<serde_json::Value>(path)?;
    let workflow: wf_types::workflow::WorkflowDefinition = serde_json::from_value(value.clone())
        .map_err(|e| ConfigError::Parse(format!("invalid workflow: {e}")))?;
    crate::processor::workflow::validate_workflow_definition(&workflow)?;
    Ok(value)
}

fn load_node_template(path: &Path) -> ConfigResult<serde_json::Value> {
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

fn load_script(path: &Path) -> ConfigResult<serde_json::Value> {
    let value = crate::parser::parse_config_file::<serde_json::Value>(path)?;
    let script: wf_types::script::executor::ScriptExecutorConfig =
        serde_json::from_value(value.clone())
            .map_err(|e| ConfigError::Parse(format!("invalid script: {e}")))?;
    crate::processor::script::validate_script_executor(&script)?;
    Ok(value)
}

fn load_prompt_template(path: &Path) -> ConfigResult<serde_json::Value> {
    let value = crate::parser::parse_config_file::<serde_json::Value>(path)?;
    let template: wf_types::template::Template = serde_json::from_value(value.clone())
        .map_err(|e| ConfigError::Parse(format!("invalid prompt template: {e}")))?;
    crate::processor::prompt::validate_prompt_template(&template)?;
    Ok(value)
}

fn load_agent_loop(path: &Path) -> ConfigResult<serde_json::Value> {
    let value = crate::parser::parse_config_file::<serde_json::Value>(path)?;
    let agent: wf_types::agent::AgentDefinition = serde_json::from_value(value.clone())
        .map_err(|e| ConfigError::Parse(format!("invalid agent loop: {e}")))?;
    crate::processor::agent_loop::validate_agent_definition(&agent)?;
    Ok(value)
}

// ── generic resolver factory ────────────────────────────────────────────

/// Create an index resolver for a specific index type.
///
/// The resolver loads the index file, expands its `paths`, loads each config
/// file through `loader` (parse + validate + transform), extracts metadata
/// via `extract`, and records failures without aborting.
pub fn create_index_resolver(
    index_type: IndexType,
    loader: IndexConfigLoader,
    extract: IndexMetadataExtractor,
) -> IndexResolver {
    Arc::new(move |index_path: &Path| {
        let index_type = index_type.clone();
        let loader = loader.clone();
        let owned_path = index_path.to_path_buf();
        Box::pin(async move {
            let index = load_index_file(&owned_path)?;
            // Glob patterns resolve relative to the index file's directory.
            let index_file_path = if owned_path.is_dir() {
                owned_path.join(crate::preset::INDEX_FILE_NAME)
            } else {
                owned_path
            };
            let base_dir = index_file_path.parent().unwrap_or_else(|| Path::new("."));
            let file_paths = expand_index_paths(&index, base_dir);

            let mut entries = Vec::new();
            let mut failures = Vec::new();
            for file_path in &file_paths {
                // Skip the index file itself when a broad pattern matched it.
                let is_index = Path::new(file_path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.eq_ignore_ascii_case(crate::preset::INDEX_FILE_NAME))
                    .unwrap_or(false);
                if is_index {
                    continue;
                }
                match loader(Path::new(file_path)) {
                    Ok(value) => entries.push(extract(&value, file_path)),
                    Err(e) => failures.push(IndexLoadFailure {
                        path: file_path.clone(),
                        error: e.to_string(),
                    }),
                }
            }
            let total_count = entries.len();
            Ok(ResolvedIndex {
                index_type,
                entries,
                metadata: Some(ResolvedIndexMetadata {
                    resolved_at: wf_common::time::timestamp_to_iso(wf_common::time::now()),
                    total_count,
                    failures,
                }),
            })
        })
    })
}

// ── preset-based index resolvers ────────────────────────────────────────

/// Resolve an MCP Presets index: one entry per configured MCP server.
pub fn resolve_mcp_presets_index(path: &Path) -> ConfigResult<ResolvedIndex> {
    let mut entries = Vec::new();
    let mut failures = Vec::new();
    match crate::mcp::load_mcp_settings(path) {
        Ok(settings) => {
            for server_id in settings.mcp_servers.keys() {
                entries.push(ResolvedIndexEntry {
                    id: server_id.clone(),
                    name: Some(server_id.clone()),
                    description: None,
                    tags: None,
                    category: None,
                    file_path: path.to_string_lossy().to_string(),
                    format: if path
                        .extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| e.eq_ignore_ascii_case("json"))
                    {
                        ConfigFileFormat::Json
                    } else {
                        ConfigFileFormat::Toml
                    },
                    metadata: None,
                });
            }
        }
        Err(e) => failures.push(IndexLoadFailure {
            path: path.to_string_lossy().to_string(),
            error: e.to_string(),
        }),
    }
    let total_count = entries.len();
    Ok(ResolvedIndex {
        index_type: IndexType::McpPresets,
        entries,
        metadata: Some(ResolvedIndexMetadata {
            resolved_at: wf_common::time::timestamp_to_iso(wf_common::time::now()),
            total_count,
            failures,
        }),
    })
}

/// Resolve a Skill Presets index: an entry for the skill collection file
/// itself plus entries for each expanded skill path.
pub fn resolve_skill_presets_index(path: &Path) -> ConfigResult<ResolvedIndex> {
    let mut entries = Vec::new();
    let mut failures = Vec::new();
    match crate::skill::load_skill_config(path) {
        Ok(Some(config)) => {
            let base_name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            entries.push(ResolvedIndexEntry {
                id: base_name.clone(),
                name: Some(base_name.clone()),
                description: Some(format!("Skill collection from {base_name}")),
                tags: None,
                category: None,
                file_path: path.to_string_lossy().to_string(),
                format: ConfigFileFormat::Json,
                metadata: None,
            });
            if !config.paths.is_empty() {
                let skill_dir = path.parent().unwrap_or_else(|| Path::new("."));
                let skill_paths = expand_skill_paths(&config.paths, skill_dir);
                for skill_path in skill_paths {
                    let skill_name = skill_path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or_default()
                        .to_string();
                    entries.push(ResolvedIndexEntry {
                        id: format!("{base_name}:{skill_name}"),
                        name: Some(skill_name),
                        description: Some(format!("Skill from {base_name}")),
                        tags: None,
                        category: None,
                        file_path: skill_path.to_string_lossy().to_string(),
                        format: ConfigFileFormat::Json,
                        metadata: None,
                    });
                }
            }
        }
        Ok(None) => failures.push(IndexLoadFailure {
            path: path.to_string_lossy().to_string(),
            error: "skill settings file not found".to_string(),
        }),
        Err(e) => failures.push(IndexLoadFailure {
            path: path.to_string_lossy().to_string(),
            error: e.to_string(),
        }),
    }
    let total_count = entries.len();
    Ok(ResolvedIndex {
        index_type: IndexType::SkillPresets,
        entries,
        metadata: Some(ResolvedIndexMetadata {
            resolved_at: wf_common::time::timestamp_to_iso(wf_common::time::now()),
            total_count,
            failures,
        }),
    })
}

/// Expand skill path patterns to actual file paths (relative to `base_dir`).
fn expand_skill_paths(patterns: &[String], base_dir: &Path) -> Vec<PathBuf> {
    let mut all_paths: Vec<PathBuf> = Vec::new();
    for pattern in patterns {
        let normalized = pattern.trim_end_matches('/');
        let normalized = if normalized.ends_with("/**") {
            format!("{normalized}/*")
        } else {
            normalized.to_string()
        };
        let full = base_dir.join(&normalized);
        let full_str = full.to_string_lossy().to_string();
        if full_str.contains('*') {
            if let Ok(matches) = crate::loader::expand_glob_paths(&full_str) {
                all_paths.extend(matches);
            }
        } else if full.exists() {
            all_paths.push(full);
        }
    }
    all_paths.sort();
    all_paths.dedup();
    all_paths
}

/// Resolve an Infrastructure Presets index: verifies the project infra
/// config loads, then emits an entry for the preset file itself.
pub fn resolve_infrastructure_presets_index(path: &Path) -> ConfigResult<ResolvedIndex> {
    let mut entries = Vec::new();
    let mut failures = Vec::new();
    let project_root = path.parent().unwrap_or_else(|| Path::new("."));
    match crate::orchestrator::ConfigOrchestrator::assemble(project_root, None) {
        Ok(_) => {
            let preset_id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            entries.push(ResolvedIndexEntry {
                id: preset_id.clone(),
                name: Some(preset_id),
                description: None,
                tags: None,
                category: None,
                file_path: path.to_string_lossy().to_string(),
                format: ConfigFileFormat::Json,
                metadata: None,
            });
        }
        Err(e) => failures.push(IndexLoadFailure {
            path: path.to_string_lossy().to_string(),
            error: e.to_string(),
        }),
    }
    let total_count = entries.len();
    Ok(ResolvedIndex {
        index_type: IndexType::InfrastructurePresets,
        entries,
        metadata: Some(ResolvedIndexMetadata {
            resolved_at: wf_common::time::timestamp_to_iso(wf_common::time::now()),
            total_count,
            failures,
        }),
    })
}

// ── resolver registry ───────────────────────────────────────────────────

/// Wrap a synchronous resolver function into an [`IndexResolver`].
fn sync_resolver(f: fn(&Path) -> ConfigResult<ResolvedIndex>) -> IndexResolver {
    Arc::new(move |path: &Path| {
        let owned = path.to_path_buf();
        Box::pin(async move { f(&owned) })
    })
}

/// Create the resolver for a supported index type.
///
/// Supported types (8 of the 10 index types; `trigger_templates` is not
/// supported).
pub fn create_index_resolver_for_type(ty: &IndexType) -> ConfigResult<IndexResolver> {
    let (loader, extract): (IndexConfigLoader, IndexMetadataExtractor) = match ty {
        IndexType::LlmProfiles => (Arc::new(load_llm_profile), extract_llm_profile_metadata),
        IndexType::Workflows => (Arc::new(load_workflow), extract_workflow_metadata),
        IndexType::NodeTemplates => (Arc::new(load_node_template), extract_node_template_metadata),
        IndexType::Scripts => (Arc::new(load_script), extract_script_metadata),
        IndexType::PromptTemplates => (
            Arc::new(load_prompt_template),
            extract_prompt_template_metadata,
        ),
        IndexType::AgentLoops => (Arc::new(load_agent_loop), extract_agent_loop_metadata),
        _ => {
            return Err(ConfigError::Index(format!(
                "unsupported index type: {}",
                ty.as_str()
            )))
        }
    };
    Ok(create_index_resolver(ty.clone(), loader, extract))
}

/// Register resolvers for all supported index types into a registry.
/// Idempotent: safe to call repeatedly.
pub fn register_all_index_resolvers(registry: &mut IndexRegistry) -> ConfigResult<()> {
    for ty in [
        IndexType::LlmProfiles,
        IndexType::Workflows,
        IndexType::NodeTemplates,
        IndexType::Scripts,
        IndexType::PromptTemplates,
        IndexType::AgentLoops,
        IndexType::McpPresets,
        IndexType::SkillPresets,
        IndexType::InfrastructurePresets,
    ] {
        let resolver = match ty {
            IndexType::McpPresets => sync_resolver(resolve_mcp_presets_index),
            IndexType::SkillPresets => sync_resolver(resolve_skill_presets_index),
            IndexType::InfrastructurePresets => sync_resolver(resolve_infrastructure_presets_index),
            _ => create_index_resolver_for_type(&ty)?,
        };
        registry.register(ty, resolver)?;
    }
    Ok(())
}

// ── filtering utilities ─────────────────────────────────────────────────

/// Filter resolved entries by tags. Tags on entries are stored as arrays;
/// all requested tags must be present.
pub fn filter_by_tags(entries: &[ResolvedIndexEntry], tags: &[String]) -> Vec<ResolvedIndexEntry> {
    if tags.is_empty() {
        return entries.to_vec();
    }
    entries
        .iter()
        .filter(|entry| {
            tags.iter().all(|tag| {
                entry
                    .tags
                    .as_ref()
                    .map(|entry_tags| entry_tags.contains(tag))
                    .unwrap_or(false)
            })
        })
        .cloned()
        .collect()
}

pub fn filter_by_category(
    entries: &[ResolvedIndexEntry],
    category: &str,
) -> Vec<ResolvedIndexEntry> {
    entries
        .iter()
        .filter(|entry| entry.category.as_deref() == Some(category))
        .cloned()
        .collect()
}

pub fn find_entry_by_id<'a>(
    entries: &'a [ResolvedIndexEntry],
    id: &str,
) -> Option<&'a ResolvedIndexEntry> {
    entries.iter().find(|entry| entry.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_json(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn test_index_type_as_str() {
        assert_eq!(IndexType::LlmProfiles.as_str(), "llm_profiles");
        assert_eq!(IndexType::Workflows.as_str(), "workflows");
    }

    #[test]
    fn test_ts_index_schema_parses() {
        // index.json uses `type` (not `index_type`).
        let path = std::env::temp_dir().join(format!("wf-index-schema-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        write_json(
            &path.join("index.json"),
            r#"{"version": "1.0", "type": "llm_profiles", "paths": ["./profiles/*.json"]}"#,
        );
        let index = load_index_file(&path).unwrap();
        assert_eq!(index.index_type, IndexType::LlmProfiles);
        assert_eq!(index.paths.len(), 1);
        let _ = std::fs::remove_dir_all(&path);
    }

    #[test]
    fn test_index_registry_idempotent() {
        let mut registry = IndexRegistry::new();
        assert!(!registry.has_resolver(&IndexType::LlmProfiles));

        let resolver: IndexResolver = Arc::new(|_path: &Path| {
            Box::pin(async {
                Ok(ResolvedIndex {
                    index_type: IndexType::LlmProfiles,
                    entries: vec![],
                    metadata: None,
                })
            })
        });

        registry
            .register(IndexType::LlmProfiles, resolver.clone())
            .unwrap();
        assert!(registry.has_resolver(&IndexType::LlmProfiles));
        // Re-registration is idempotent (no error).
        registry.register(IndexType::LlmProfiles, resolver).unwrap();
        assert_eq!(registry.registered_types().len(), 1);
    }

    #[test]
    fn test_register_all_index_resolvers() {
        let mut registry = IndexRegistry::new();
        register_all_index_resolvers(&mut registry).unwrap();
        assert_eq!(registry.registered_types().len(), 9);
        // Idempotent: calling again keeps the same set.
        register_all_index_resolvers(&mut registry).unwrap();
        assert_eq!(registry.registered_types().len(), 9);
        assert!(registry.has_resolver(&IndexType::McpPresets));
    }

    fn make_entry(id: &str, tags: Option<&[&str]>, category: Option<&str>) -> ResolvedIndexEntry {
        ResolvedIndexEntry {
            id: id.to_string(),
            name: None,
            description: None,
            tags: tags.map(|t| t.iter().map(|s| s.to_string()).collect()),
            category: category.map(str::to_string),
            file_path: format!("/config/{id}.toml"),
            format: ConfigFileFormat::Toml,
            metadata: None,
        }
    }

    #[test]
    fn test_filter_by_tags() {
        let entries = vec![
            make_entry("a", Some(&["review", "code"]), Some("system")),
            make_entry("b", Some(&["test"]), Some("user")),
            make_entry("c", Some(&["review", "test"]), Some("system")),
        ];

        let result = filter_by_tags(&entries, &["review".to_string()]);
        assert_eq!(result.len(), 2);

        let result = filter_by_tags(&entries, &["review".to_string(), "test".to_string()]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "c");

        let result = filter_by_tags(&entries, &[]);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_filter_by_category() {
        let entries = vec![
            make_entry("a", None, Some("system")),
            make_entry("b", None, Some("user")),
            make_entry("c", None, Some("system")),
        ];

        let result = filter_by_category(&entries, "system");
        assert_eq!(result.len(), 2);

        let result = filter_by_category(&entries, "nonexistent");
        assert!(result.is_empty());
    }

    #[test]
    fn test_find_entry_by_id() {
        let entries = vec![make_entry("a", None, None), make_entry("b", None, None)];

        assert!(find_entry_by_id(&entries, "a").is_some());
        assert!(find_entry_by_id(&entries, "b").is_some());
        assert!(find_entry_by_id(&entries, "c").is_none());
    }

    #[test]
    fn test_expand_index_paths_literal() {
        let index = ConfigIndexFile {
            version: "1.0".to_string(),
            index_type: IndexType::LlmProfiles,
            paths: vec!["profiles/openai.toml".to_string()],
        };
        let base = Path::new("/config");
        let result = expand_index_paths(&index, base);
        assert_eq!(result, vec!["/config/profiles/openai.toml".to_string()]);
    }

    #[test]
    fn test_metadata_extraction() {
        let llm = serde_json::json!({
            "id": "p1", "name": "Profile One", "description": "desc",
            "tags": ["code", "review"], "provider": "openai", "model": "gpt-4"
        });
        let entry = extract_llm_profile_metadata(&llm, "/x/p1.json");
        assert_eq!(entry.id, "p1");
        assert_eq!(
            entry.tags,
            Some(vec!["code".to_string(), "review".to_string()])
        );
        let extra = entry.metadata.as_ref().unwrap();
        assert_eq!(extra["provider"], "openai");
        assert_eq!(extra["model"], "gpt-4");
        assert_eq!(entry.format, ConfigFileFormat::Json);

        let wf = serde_json::json!({
            "id": "w1", "name": "WF", "type": "batch", "version": "1.2",
            "metadata": {"author": "alice", "tags": ["a", "b"]}
        });
        let entry = extract_workflow_metadata(&wf, "/x/w1.toml");
        assert_eq!(entry.id, "w1");
        assert_eq!(entry.metadata.as_ref().unwrap()["version"], "1.2");
        assert_eq!(entry.metadata.as_ref().unwrap()["author"], "alice");
        assert_eq!(entry.tags, Some(vec!["a".to_string(), "b".to_string()]));
        assert_eq!(entry.format, ConfigFileFormat::Toml);

        let nt = serde_json::json!({"name": "nt1", "description": "d", "node_type": "LLM"});
        let entry = extract_node_template_metadata(&nt, "/x/nt1.json");
        assert_eq!(entry.id, "nt1");
        assert_eq!(entry.metadata.as_ref().unwrap()["node_type"], "LLM");

        let script = serde_json::json!({"id": "s1", "name": "s", "category": "util"});
        let entry = extract_script_metadata(&script, "/x/s1.json");
        assert_eq!(entry.id, "s1");
        assert_eq!(entry.category.as_deref(), Some("util"));
    }

    #[test]
    fn test_resolver_collects_failures() {
        let dir = std::env::temp_dir().join(format!("wf-index-resolve-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        write_json(
            &dir.join("index.json"),
            r#"{"version": "1.0", "type": "llm_profiles", "paths": ["./*.json"]}"#,
        );
        write_json(
            &dir.join("good.json"),
            r#"{"id": "g1", "name": "G", "provider": "OPENAI", "model": "m1"}"#,
        );
        write_json(&dir.join("bad.json"), r#"{invalid json"#);

        let resolver = create_index_resolver_for_type(&IndexType::LlmProfiles).unwrap();
        let resolved = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(resolver(&dir))
            .unwrap();
        assert_eq!(resolved.index_type, IndexType::LlmProfiles);
        assert_eq!(resolved.entries.len(), 1);
        assert_eq!(resolved.entries[0].id, "g1");
        let metadata = resolved.metadata.unwrap();
        assert_eq!(metadata.total_count, 1);
        assert_eq!(metadata.failures.len(), 1);
        assert!(metadata.failures[0].path.ends_with("bad.json"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_preset_index_resolvers() {
        let dir = std::env::temp_dir().join(format!("wf-index-preset-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        // MCP preset index: entries per server.
        write_json(
            &dir.join("mcp.json"),
            r#"{"mcpServers": {"s1": {"type": "stdio", "command": "x"}, "s2": {"type": "stdio", "command": "y"}}}"#,
        );
        let resolved = resolve_mcp_presets_index(&dir.join("mcp.json")).unwrap();
        assert_eq!(resolved.entries.len(), 2);
        let mut ids: Vec<&str> = resolved.entries.iter().map(|e| e.id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, vec!["s1", "s2"]);

        // Skill preset index: collection entry + expanded skills.
        let skills = dir.join("skills");
        std::fs::create_dir_all(skills.join("sdir")).unwrap();
        write_json(
            &skills.join("default.json"),
            r#"{"paths": ["./sdir/*.json"]}"#,
        );
        write_json(&skills.join("sdir").join("one.json"), r#"{"id": "one"}"#);
        let resolved = resolve_skill_presets_index(&skills.join("default.json")).unwrap();
        assert_eq!(resolved.entries.len(), 2);
        assert_eq!(resolved.entries[1].id, "default:one");

        // Infrastructure preset index: entry emitted when assembly succeeds.
        let resolved = resolve_infrastructure_presets_index(&dir.join("dev.json")).unwrap();
        assert_eq!(resolved.entries.len(), 1);
        assert_eq!(resolved.entries[0].id, "dev");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_registry_resolve_uses_registered_resolver() {
        let dir = std::env::temp_dir().join(format!("wf-index-registry-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        write_json(
            &dir.join("index.json"),
            r#"{"version": "1.0", "type": "mcp_presets", "paths": ["./*.json"]}"#,
        );
        write_json(
            &dir.join("default.json"),
            r#"{"mcpServers": {"a": {"type": "stdio", "command": "x"}}}"#,
        );

        let mut registry = IndexRegistry::new();
        register_all_index_resolvers(&mut registry).unwrap();
        let resolved = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(registry.resolve(&IndexType::McpPresets, &dir.join("default.json")))
            .unwrap();
        assert_eq!(resolved.entries.len(), 1);
        assert_eq!(resolved.entries[0].id, "a");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
