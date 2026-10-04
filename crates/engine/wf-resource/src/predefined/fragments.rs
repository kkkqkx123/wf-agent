use serde::Deserialize;
use std::path::Path;
use wf_core::MutableRegistry;
use wf_types::SystemPromptFragment;

use crate::registry::{
    register_item_skip, register_item_strict, RegisterOptions, ResourceRegistries,
};
use crate::result::Summary;

#[derive(Debug, Deserialize)]
struct FragmentsFile {
    fragments: Vec<FragmentEntry>,
}

#[derive(Debug, Deserialize)]
struct FragmentEntry {
    id: String,
    category: String,
    content: String,
    description: Option<String>,
}

fn embedded_fragments() -> FragmentsFile {
    serde_json::from_str(include_str!("../../../../../configs/predefined/fragments.json"))
        .expect("embedded fragments.json is valid")
}

fn fragment_from_entry(entry: FragmentEntry) -> SystemPromptFragment {
    SystemPromptFragment {
        id: entry.id,
        category: entry.category,
        content: entry.content,
        description: entry.description,
        variables: None,
    }
}

pub fn builtin_fragments() -> Vec<SystemPromptFragment> {
    embedded_fragments()
        .fragments
        .into_iter()
        .map(fragment_from_entry)
        .collect()
}

fn load_override_fragments(path: &Path) -> Result<Vec<FragmentEntry>, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read fragments config {}: {}", path.display(), e))?;
    serde_json::from_str::<FragmentsFile>(&content)
        .map(|file| file.fragments)
        .map_err(|e| format!("failed to parse fragments config {}: {}", path.display(), e))
}

pub fn register(regs: &ResourceRegistries, opts: &RegisterOptions) -> Summary {
    let mut total = Summary::new();

    for fragment in builtin_fragments() {
        let id = fragment.id.clone();
        total.merge(if opts.skip_if_exists {
            register_item_skip(&regs.fragments, id, fragment)
        } else {
            register_item_strict(&regs.fragments, id, fragment)
        });
    }

    if let Some(ref path_str) = opts.fragments_config_path {
        let path = Path::new(path_str);
        match load_override_fragments(path) {
            Ok(entries) => {
                for entry in entries {
                    if !wf_types::is_valid_fragment_category(&entry.category) {
                        total.merge(Summary::err(
                            &entry.id,
                            format!(
                                "invalid fragment category '{}' (allowed: {})",
                                entry.category,
                                wf_types::FRAGMENT_CATEGORIES.join(", ")
                            ),
                        ));
                        continue;
                    }
                    if entry.content.trim().is_empty() {
                        total.merge(Summary::err(&entry.id, "fragment has empty content"));
                        continue;
                    }
                    let id = entry.id.clone();
                    let fragment = fragment_from_entry(entry);
                    regs.fragments.unregister(&id);
                    total.merge(register_item_strict(&regs.fragments, id, fragment));
                }
            }
            Err(e) => {
                total.merge(Summary::err("fragments_config_path", e));
            }
        }
    }

    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use wf_core::Registry;

    fn write_tmp(content: &str) -> tempfile::NamedTempFile {
        let mut file = tempfile::Builder::new()
            .suffix(".json")
            .tempfile()
            .unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file
    }

    #[test]
    fn embedded_fragments_load() {
        let fragments = builtin_fragments();
        assert_eq!(fragments.len(), 14);
        assert!(fragments.iter().any(|f| f.id == "fragments.role.assistant"));
        assert!(fragments.iter().any(|f| f.id == "fragments.constraint.code-safety"));
    }

    #[test]
    fn all_categories_valid() {
        for fragment in builtin_fragments() {
            assert!(
                wf_types::is_valid_fragment_category(&fragment.category),
                "fragment '{}' has invalid category '{}'",
                fragment.id,
                fragment.category
            );
        }
    }

    #[test]
    fn override_replaces_embedded_default() {
        let file = write_tmp(
            r#"{"fragments": [{"id": "fragments.role.assistant", "category": "role", "content": "Override content", "description": null}]}"#,
        );
        let regs = ResourceRegistries::new();
        let opts = RegisterOptions {
            fragments_config_path: Some(file.path().to_string_lossy().to_string()),
            ..Default::default()
        };
        let summary = register(&regs, &opts);
        assert!(summary.failed.is_empty(), "{:?}", summary.failed);
        let fragment = regs.fragments.get("fragments.role.assistant").unwrap();
        assert_eq!(fragment.content, "Override content");
    }

    #[test]
    fn override_rejects_invalid_category() {
        let file = write_tmp(
            r#"{"fragments": [{"id": "fragments.bad", "category": "invalid", "content": "Content", "description": null}]}"#,
        );
        let regs = ResourceRegistries::new();
        let opts = RegisterOptions {
            fragments_config_path: Some(file.path().to_string_lossy().to_string()),
            ..Default::default()
        };
        let summary = register(&regs, &opts);
        assert!(summary.failed.iter().any(|f| f.id == "fragments.bad"));
        assert!(!regs.fragments.has("fragments.bad"));
    }

    #[test]
    fn override_rejects_empty_content() {
        let file = write_tmp(
            r#"{"fragments": [{"id": "fragments.empty", "category": "role", "content": "   ", "description": null}]}"#,
        );
        let regs = ResourceRegistries::new();
        let opts = RegisterOptions {
            fragments_config_path: Some(file.path().to_string_lossy().to_string()),
            ..Default::default()
        };
        let summary = register(&regs, &opts);
        assert!(summary.failed.iter().any(|f| f.id == "fragments.empty"));
        assert!(!regs.fragments.has("fragments.empty"));
    }

    #[test]
    fn override_missing_file_reports_error() {
        let regs = ResourceRegistries::new();
        let opts = RegisterOptions {
            fragments_config_path: Some("/nonexistent/fragments.json".to_string()),
            ..Default::default()
        };
        let summary = register(&regs, &opts);
        assert!(summary.failed.iter().any(|f| f.id == "fragments_config_path"));
    }

    #[test]
    fn override_adds_new_fragments() {
        let file = write_tmp(
            r#"{"fragments": [{"id": "fragments.custom.new", "category": "role", "content": "Custom fragment", "description": "New"}]}"#,
        );
        let regs = ResourceRegistries::new();
        let opts = RegisterOptions {
            fragments_config_path: Some(file.path().to_string_lossy().to_string()),
            ..Default::default()
        };
        let summary = register(&regs, &opts);
        assert!(summary.failed.is_empty(), "{:?}", summary.failed);
        assert!(regs.fragments.has("fragments.custom.new"));
    }
}
