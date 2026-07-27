use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::error::{ConfigError, ConfigResult};

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum IndexType {
    LlmProfiles,
    Workflows,
    NodeTemplates,
    TriggerTemplates,
    HookTemplates,
    Scripts,
    PromptTemplates,
    AgentLoops,
    McpPresets,
    SkillPresets,
    InfrastructurePresets,
}

impl IndexType {
    pub fn as_str(&self) -> &'static str {
        match self {
            IndexType::LlmProfiles => "llm_profiles",
            IndexType::Workflows => "workflows",
            IndexType::NodeTemplates => "node_templates",
            IndexType::TriggerTemplates => "trigger_templates",
            IndexType::HookTemplates => "hook_templates",
            IndexType::Scripts => "scripts",
            IndexType::PromptTemplates => "prompt_templates",
            IndexType::AgentLoops => "agent_loops",
            IndexType::McpPresets => "mcp_presets",
            IndexType::SkillPresets => "skill_presets",
            IndexType::InfrastructurePresets => "infrastructure_presets",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConfigIndexFile {
    pub version: String,
    pub index_type: IndexType,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedIndexEntry {
    pub path: String,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedIndex {
    pub index_type: IndexType,
    pub entries: Vec<ResolvedIndexEntry>,
    pub total_count: usize,
    pub failures: Vec<String>,
}

pub type IndexResolver = Arc<
    dyn Fn(&Path) -> std::pin::Pin<Box<dyn std::future::Future<Output = ConfigResult<ResolvedIndex>> + Send>>
        + Send
        + Sync,
>;

pub struct IndexRegistry {
    resolvers: HashMap<IndexType, IndexResolver>,
}

impl IndexRegistry {
    pub fn new() -> Self {
        Self {
            resolvers: HashMap::new(),
        }
    }

    pub fn register(&mut self, ty: IndexType, resolver: IndexResolver) -> ConfigResult<()> {
        if self.resolvers.contains_key(&ty) {
            return Err(ConfigError::Index(format!(
                "resolver for {:?} is already registered",
                ty
            )));
        }
        self.resolvers.insert(ty, resolver);
        Ok(())
    }

    pub async fn resolve(&self, ty: &IndexType, path: &Path) -> ConfigResult<ResolvedIndex> {
        let resolver = self.resolvers.get(ty).ok_or_else(|| {
            ConfigError::Index(format!("no resolver registered for {:?}", ty))
        })?;
        resolver(path).await
    }

    pub fn has_resolver(&self, ty: &IndexType) -> bool {
        self.resolvers.contains_key(ty)
    }
}

impl Default for IndexRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub fn load_index_file(path: &Path) -> ConfigResult<ConfigIndexFile> {
    crate::parser::parse_config_file(path)
}

pub fn filter_by_tags(entries: &[ResolvedIndexEntry], tags: &[String]) -> Vec<ResolvedIndexEntry> {
    if tags.is_empty() {
        return entries.to_vec();
    }
    entries
        .iter()
        .filter(|entry| {
            entry
                .metadata
                .get("tags")
                .map(|t| {
                    let entry_tags: Vec<&str> = t.split(',').map(str::trim).collect();
                    tags.iter().all(|tag| entry_tags.contains(&tag.as_str()))
                })
                .unwrap_or(false)
        })
        .cloned()
        .collect()
}

pub fn filter_by_category(entries: &[ResolvedIndexEntry], category: &str) -> Vec<ResolvedIndexEntry> {
    entries
        .iter()
        .filter(|entry| entry.metadata.get("category").map(|c| c.as_str()) == Some(category))
        .cloned()
        .collect()
}

pub fn find_entry_by_id<'a>(
    entries: &'a [ResolvedIndexEntry],
    id: &str,
) -> Option<&'a ResolvedIndexEntry> {
    entries
        .iter()
        .find(|entry| entry.metadata.get("id").map(|i| i.as_str()) == Some(id))
}

pub fn expand_index_paths(index: &ConfigIndexFile, base_dir: &Path) -> Vec<String> {
    let mut result = Vec::new();
    for pattern in &index.paths {
        if pattern.contains('*') {
            let full_pattern = base_dir.join(pattern);
            let pattern_str = full_pattern.to_string_lossy();
            if let Ok(entries) = glob::glob(&pattern_str) {
                for entry in entries.flatten() {
                    result.push(entry.to_string_lossy().to_string());
                }
            }
        } else {
            result.push(base_dir.join(pattern).to_string_lossy().to_string());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_type_as_str() {
        assert_eq!(IndexType::LlmProfiles.as_str(), "llm_profiles");
        assert_eq!(IndexType::Workflows.as_str(), "workflows");
    }

    #[test]
    fn test_index_registry() {
        let mut registry = IndexRegistry::new();
        assert!(!registry.has_resolver(&IndexType::LlmProfiles));

        let resolver: IndexResolver = Arc::new(|_path: &Path| {
            Box::pin(async {
                Ok(ResolvedIndex {
                    index_type: IndexType::LlmProfiles,
                    entries: vec![],
                    total_count: 0,
                    failures: vec![],
                })
            })
        });

        registry.register(IndexType::LlmProfiles, resolver).unwrap();
        assert!(registry.has_resolver(&IndexType::LlmProfiles));

        let duplicate: IndexResolver = Arc::new(|_path: &Path| {
            Box::pin(async {
                Ok(ResolvedIndex {
                    index_type: IndexType::LlmProfiles,
                    entries: vec![],
                    total_count: 0,
                    failures: vec![],
                })
            })
        });
        assert!(
            registry
                .register(IndexType::LlmProfiles, duplicate)
                .is_err()
        );
    }

    fn make_entry(id: &str, tags: Option<&str>, category: Option<&str>) -> ResolvedIndexEntry {
        let mut metadata = HashMap::new();
        metadata.insert("id".to_string(), id.to_string());
        if let Some(t) = tags {
            metadata.insert("tags".to_string(), t.to_string());
        }
        if let Some(c) = category {
            metadata.insert("category".to_string(), c.to_string());
        }
        ResolvedIndexEntry {
            path: format!("/config/{id}.toml"),
            metadata,
        }
    }

    #[test]
    fn test_filter_by_tags() {
        let entries = vec![
            make_entry("a", Some("review,code"), Some("system")),
            make_entry("b", Some("test"), Some("user")),
            make_entry("c", Some("review,test"), Some("system")),
        ];

        let result = filter_by_tags(&entries, &["review".to_string()]);
        assert_eq!(result.len(), 2);

        let result = filter_by_tags(&entries, &["review".to_string(), "test".to_string()]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].metadata.get("id").unwrap(), "c");

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
        let entries = vec![
            make_entry("a", None, None),
            make_entry("b", None, None),
        ];

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
}
