//! Index manifests for one-record-per-file config families.
//!
//! An index only carries path patterns. Metadata comes from the individual
//! record files, and the resource families themselves are registered through
//! the API and storage layers.

mod load;

pub use load::*;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use wf_types::config::{ConfigIndexFile, IndexType};

    fn write_json(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn test_index_schema_parses() {
        let path = std::env::temp_dir().join(format!("wf-index-schema-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
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
        write_json(
            &path.join("index.json"),
            r#"{"version": "2.0", "type": "llm_profiles", "paths": ["./*.json"]}"#,
        );
        assert!(load_index_file(&path).is_err());
        let _ = std::fs::remove_dir_all(&path);
    }

    #[test]
    fn test_expand_index_paths_skips_examples() {
        let path = std::env::temp_dir().join(format!("wf-index-examples-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        write_json(
            &path.join("index.json"),
            r#"{"version": "1.0", "type": "llm_auth", "paths": ["./*.toml"]}"#,
        );
        write_json(&path.join("custom.auth.toml"), "id = \"x\"\n");
        write_json(&path.join("sample.auth.example.toml"), "id = \"y\"\n");
        let index = load_index_file(&path).unwrap();
        let expanded = expand_index_paths(&index, &path);
        assert_eq!(expanded.len(), 1);
        assert!(expanded[0].ends_with("custom.auth.toml"));
        let _ = std::fs::remove_dir_all(&path);
    }

    #[test]
    fn test_expand_index_paths_literal() {
        let index = ConfigIndexFile {
            version: "1.0".to_string(),
            index_type: IndexType::LlmProfiles,
            paths: vec!["profiles/openai.toml".to_string()],
        };
        let expanded = expand_index_paths(&index, Path::new("/config"));
        assert_eq!(expanded, vec!["/config/profiles/openai.toml".to_string()]);
    }
}
