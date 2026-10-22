use super::*;

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
pub(crate) fn expand_skill_paths(patterns: &[String], base_dir: &Path) -> Vec<PathBuf> {
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
pub(crate) fn sync_resolver(f: fn(&Path) -> ConfigResult<ResolvedIndex>) -> IndexResolver {
    Arc::new(move |path: &Path| {
        let owned = path.to_path_buf();
        Box::pin(async move { f(&owned) })
    })
}

/// Create the resolver for a supported index type.
///
/// Supported types (10 of the 11 index types; `trigger_templates` is not
/// supported).
pub fn create_index_resolver_for_type(ty: &IndexType) -> ConfigResult<IndexResolver> {
    let (loader, extract): (IndexConfigLoader, IndexMetadataExtractor) = match ty {
        IndexType::LlmProfiles => (Arc::new(load_llm_profile), extract_llm_profile_metadata),
        IndexType::LlmProviders => (
            Arc::new(load_llm_provider_definition),
            extract_llm_provider_metadata,
        ),
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
        IndexType::LlmProviders,
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
