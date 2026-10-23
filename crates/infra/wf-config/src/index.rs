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

mod filter;
mod load;
mod metadata;
mod registry;
mod resolvers;

pub use filter::*;
pub use load::*;
pub use metadata::*;
pub use registry::*;
pub use resolvers::*;

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
    fn test_index_file_rejects_unexpected_version() {
        let path = std::env::temp_dir().join(format!("wf-index-version-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        write_json(
            &path.join("index.json"),
            r#"{"version": "2.0", "type": "llm_profiles", "paths": ["./profiles/*.json"]}"#,
        );
        assert!(load_index_file(&path).is_err());
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
        assert_eq!(registry.registered_types().len(), 10);
        // Idempotent: calling again keeps the same set.
        register_all_index_resolvers(&mut registry).unwrap();
        assert_eq!(registry.registered_types().len(), 10);
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
            "tags": ["code", "review"], "format": "OPENAI_CHAT",
            "provider_id": "acme", "model": "gpt-4"
        });
        let entry = extract_llm_profile_metadata(&llm, "/x/p1.json");
        assert_eq!(entry.id, "p1");
        assert_eq!(
            entry.tags,
            Some(vec!["code".to_string(), "review".to_string()])
        );
        let extra = entry.metadata.as_ref().unwrap();
        assert_eq!(extra["format"], "OPENAI_CHAT");
        assert_eq!(extra["provider_id"], "acme");
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
            r#"{"id": "g1", "name": "G", "format": "OPENAI_CHAT", "model": "m1"}"#,
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
