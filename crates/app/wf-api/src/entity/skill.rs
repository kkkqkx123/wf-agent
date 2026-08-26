//! Skill resource management.
//!
//! Backed by the shared `ToolRegistry` skill loader; when no loader is wired
//! into the context (no skill paths configured), the metadata queries degrade
//! to empty results instead of erroring.

use std::sync::Arc;

use serde::Serialize;

use wf_tools::SkillLoader;
use wf_types::skill::{SkillMetadata, SkillResourceType};

use crate::infra::context::ApiContext;
use crate::infra::error::{not_found, ApiError, ApiResult};

/// Skill filter.
#[derive(Debug, Clone, Default)]
pub struct SkillFilter {
    /// Fuzzy name match.
    pub name: Option<String>,
    /// Exact version match.
    pub version: Option<String>,
    /// Every listed tag must appear among the skill's metadata values.
    pub tags: Option<Vec<String>>,
}

/// One skill resource file (path -> content).
#[derive(Debug, Clone, Serialize)]
pub struct SkillResourceEntry {
    pub path: String,
    pub content: String,
}

/// Resolve the shared skill loader; errors with a descriptive message
/// when no loader is configured.
fn loader(ctx: &ApiContext) -> ApiResult<Arc<SkillLoader>> {
    ctx.tool_registry.skill_loader().ok_or_else(|| {
        ApiError::execution("Skill system is not available: no skill loader is configured")
    })
}

/// Whether the skill loader is present in this context.
pub fn is_available(ctx: &ApiContext) -> bool {
    ctx.tool_registry.skill_loader().is_some()
}

// ── metadata queries ────────────────────────────────────────────

/// Metadata of all known skills (empty when no loader is configured).
pub fn list_skills(ctx: &ApiContext) -> ApiResult<Vec<SkillMetadata>> {
    match ctx.tool_registry.skill_loader() {
        Some(loader) => Ok(loader.list_skills()),
        None => Ok(Vec::new()),
    }
}

pub fn get_skill(ctx: &ApiContext, name: &str) -> ApiResult<SkillMetadata> {
    let loader = loader(ctx)?;
    loader
        .get_skill(name)
        .ok_or_else(|| not_found("skill", name))
}

pub fn has_skill(ctx: &ApiContext, name: &str) -> bool {
    ctx.tool_registry
        .skill_loader()
        .map(|loader| loader.has_skill(name))
        .unwrap_or(false)
}

pub fn get_enabled_skills(ctx: &ApiContext) -> ApiResult<Vec<SkillMetadata>> {
    let loader = loader(ctx)?;
    Ok(loader.get_enabled_skills())
}

pub fn get_disabled_skills(ctx: &ApiContext) -> ApiResult<Vec<SkillMetadata>> {
    let loader = loader(ctx)?;
    Ok(loader.get_disabled_skills())
}

/// Filter skills by name / version / tags.
pub fn query(ctx: &ApiContext, filter: &SkillFilter) -> ApiResult<Vec<SkillMetadata>> {
    Ok(list_skills(ctx)?
        .into_iter()
        .filter(|skill| {
            if let Some(name) = &filter.name {
                if !skill.name.contains(name.as_str()) {
                    return false;
                }
            }
            if let Some(version) = &filter.version {
                if skill.version.as_deref() != Some(version.as_str()) {
                    return false;
                }
            }
            if let Some(tags) = &filter.tags {
                if !tags.is_empty() {
                    let values: Vec<&String> = skill
                        .metadata
                        .as_ref()
                        .map(|m| m.values().collect())
                        .unwrap_or_default();
                    if !tags
                        .iter()
                        .all(|tag| values.iter().any(|v| v.as_str() == tag))
                    {
                        return false;
                    }
                }
            }
            true
        })
        .collect())
}

// ── enable / disable ────────────────────────────────────────────

pub fn enable(ctx: &ApiContext, name: &str) -> ApiResult<()> {
    let loader = loader(ctx)?;
    loader.enable_skill(name)?;
    Ok(())
}

pub fn disable(ctx: &ApiContext, name: &str) -> ApiResult<()> {
    let loader = loader(ctx)?;
    loader.disable_skill(name)?;
    Ok(())
}

pub fn is_enabled(ctx: &ApiContext, name: &str) -> ApiResult<bool> {
    let loader = loader(ctx)?;
    Ok(loader.is_skill_enabled(name))
}

// ── cache ───────────────────────────────────────────────────────

/// Clear the content/resource caches of the skill loader.
pub fn clear_cache(ctx: &ApiContext) -> ApiResult<()> {
    let loader = loader(ctx)?;
    loader.clear_cache();
    Ok(())
}

/// Clear the cached content/resource entries of one skill only.
pub fn clear_cache_by_name(ctx: &ApiContext, name: &str) -> ApiResult<()> {
    let loader = loader(ctx)?;
    loader.clear_cache_for(name);
    Ok(())
}

// ── scanning / reload ───────────────────────────────────────────

/// Scan a skills root directory for new skill definitions (every subdirectory
/// containing `SKILL.md`) and return the updated skill list.
pub fn scan_skills(ctx: &ApiContext, dir: &str) -> ApiResult<Vec<SkillMetadata>> {
    let loader = loader(ctx)?;
    loader.scan_path(std::path::Path::new(dir));
    Ok(loader.list_skills())
}

/// Reload a skills root directory: clear the caches, rescan the directory and
/// return the refreshed skill list.
pub fn reload(ctx: &ApiContext, dir: &str) -> ApiResult<Vec<SkillMetadata>> {
    let loader = loader(ctx)?;
    loader.clear_cache();
    loader.scan_path(std::path::Path::new(dir));
    Ok(loader.list_skills())
}

/// A prompt assembled from the enabled skills: the metadata prompt followed
/// by the full body content of every enabled skill.
pub fn to_prompt(ctx: &ApiContext) -> ApiResult<String> {
    let loader = loader(ctx)?;
    let enabled = loader.get_enabled_skills();
    let mut prompt = wf_tools::skill::generate_skill_metadata_prompt(&enabled);
    for skill in &enabled {
        if let Ok(content) = loader.load_content(&skill.name) {
            prompt.push_str(&format!("\n\n## Skill: {}\n{}", skill.name, content));
        }
    }
    Ok(prompt)
}

// ── progressive disclosure ──────────────────────────────────────

/// Level 1: metadata prompt listing the enabled skills.
pub fn generate_metadata_prompt(ctx: &ApiContext) -> ApiResult<String> {
    let loader = loader(ctx)?;
    Ok(wf_tools::skill::generate_skill_metadata_prompt(
        &loader.get_enabled_skills(),
    ))
}

/// Level 1: inject the skill metadata prompt into a system prompt.
pub fn inject_skill_metadata(ctx: &ApiContext, system_prompt: &str) -> ApiResult<String> {
    let loader = loader(ctx)?;
    Ok(wf_tools::skill::inject_skill_metadata(
        system_prompt,
        &loader.get_enabled_skills(),
    ))
}

/// Level 2: load the full skill body content.
pub fn load_content(ctx: &ApiContext, name: &str) -> ApiResult<String> {
    let loader = loader(ctx)?;
    loader.load_content(name).map_err(Into::into)
}

/// Level 3: list the relative paths of a skill resource directory.
pub fn list_resources(
    ctx: &ApiContext,
    name: &str,
    resource_type: SkillResourceType,
) -> ApiResult<Vec<String>> {
    let loader = loader(ctx)?;
    Ok(loader.list_skill_resources(name, resource_type))
}

/// Level 3: load one resource file of a skill.
pub fn load_resource(
    ctx: &ApiContext,
    name: &str,
    resource_type: SkillResourceType,
    path: &str,
) -> ApiResult<String> {
    let loader = loader(ctx)?;
    match loader.load_skill_resource(name, resource_type, path)? {
        wf_tools::skill::ResourceContent::Text(text) => Ok(text),
        wf_tools::skill::ResourceContent::Binary(bytes) => {
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        }
    }
}

/// Level 3: load all resources of a skill directory.
pub fn load_resources(
    ctx: &ApiContext,
    name: &str,
    resource_type: SkillResourceType,
) -> ApiResult<Vec<SkillResourceEntry>> {
    let loader = loader(ctx)?;
    let contents = loader.load_resources(name, resource_type)?;
    Ok(contents
        .into_iter()
        .map(|(path, content)| SkillResourceEntry {
            path,
            content: match content {
                wf_tools::skill::ResourceContent::Text(text) => text,
                wf_tools::skill::ResourceContent::Binary(bytes) => {
                    String::from_utf8_lossy(&bytes).into_owned()
                }
            },
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use wf_resource::registry::ResourceRegistries;
    use wf_resource::resource_plugin::ResourcePluginRegistry;
    use wf_storage::context::StorageContext;
    use wf_types::skill::SkillConfig;

    const SKILL_MD: &str =
        "---\nname: test-skill\ndescription: A test skill\n---\n\n# Test skill body";

    /// A per-test skill directory so parallel tests never share a path.
    fn make_ctx(tag: &str) -> Arc<ApiContext> {
        let dir = std::env::temp_dir().join(format!("wf-api-skill-{}-{}", tag, std::process::id()));
        let skill_dir = dir.join("test-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), SKILL_MD).unwrap();
        std::fs::create_dir_all(skill_dir.join("references")).unwrap();
        std::fs::write(skill_dir.join("references/guide.md"), "reference content").unwrap();

        let loader = Arc::new(SkillLoader::new(SkillConfig {
            paths: vec![dir.to_string_lossy().to_string()],
            auto_scan: Some(true),
        }));
        assert_eq!(loader.list_skills().len(), 1);

        let ctx = ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
            Arc::new(ResourcePluginRegistry::new()),
        );
        ctx.tool_registry.set_skill_loader(loader);
        Arc::new(ctx)
    }

    fn cleanup(tag: &str) {
        let dir = std::env::temp_dir().join(format!("wf-api-skill-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn skill_metadata_query_and_filter() {
        let ctx = make_ctx("meta");

        let all = list_skills(&ctx).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "test-skill");
        assert!(has_skill(&ctx, "test-skill"));

        let matched = query(
            &ctx,
            &SkillFilter {
                name: Some("test".into()),
                ..SkillFilter::default()
            },
        )
        .unwrap();
        assert_eq!(matched.len(), 1);

        let err = get_skill(&ctx, "nope").unwrap_err();
        assert!(matches!(err, ApiError::NotFound { .. }));

        cleanup("meta");
    }

    #[tokio::test]
    async fn skill_enable_disable() {
        let ctx = make_ctx("enable");

        assert!(is_enabled(&ctx, "test-skill").unwrap());
        disable(&ctx, "test-skill").unwrap();
        assert!(!is_enabled(&ctx, "test-skill").unwrap());
        assert_eq!(get_disabled_skills(&ctx).unwrap().len(), 1);
        assert!(get_enabled_skills(&ctx).unwrap().is_empty());
        enable(&ctx, "test-skill").unwrap();
        assert!(is_enabled(&ctx, "test-skill").unwrap());

        let err = enable(&ctx, "missing-skill").unwrap_err();
        assert!(matches!(err, ApiError::Execution { .. }));

        cleanup("enable");
    }

    #[tokio::test]
    async fn skill_progressive_disclosure() {
        let ctx = make_ctx("disclosure");

        let prompt = generate_metadata_prompt(&ctx).unwrap();
        assert!(prompt.contains("test-skill"));

        let injected = inject_skill_metadata(&ctx, "You are a helper.").unwrap();
        assert!(injected.contains("test-skill"));

        let content = load_content(&ctx, "test-skill").unwrap();
        assert!(content.contains("Test skill body"));

        let resources = list_resources(&ctx, "test-skill", SkillResourceType::References).unwrap();
        assert!(resources.iter().any(|p| p.ends_with("guide.md")));

        let guide = load_resource(
            &ctx,
            "test-skill",
            SkillResourceType::References,
            "guide.md",
        )
        .unwrap();
        assert_eq!(guide, "reference content");

        let all = load_resources(&ctx, "test-skill", SkillResourceType::References).unwrap();
        assert_eq!(all.len(), 1);

        cleanup("disclosure");
    }

    #[tokio::test]
    async fn unavailable_loader_degrades() {
        let ctx = Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
            Arc::new(ResourcePluginRegistry::new()),
        ));
        assert!(!is_available(&ctx));
        assert!(list_skills(&ctx).unwrap().is_empty());
        let err = get_skill(&ctx, "x").unwrap_err();
        assert!(matches!(err, ApiError::Execution { .. }));
        let _ = PathBuf::new();
    }

    #[tokio::test]
    async fn scan_reload_to_prompt_and_per_name_cache_clear() {
        let ctx = make_ctx("scan");

        // A second skill is picked up by scanning its root directory.
        let dir = std::env::temp_dir().join(format!("wf-api-skill-scan-{}", std::process::id()));
        let second = dir.join("second-skill");
        std::fs::create_dir_all(&second).unwrap();
        std::fs::write(
            second.join("SKILL.md"),
            "---\nname: second-skill\ndescription: A second skill\n---\n\nBody two",
        )
        .unwrap();

        let scanned = scan_skills(&ctx, dir.to_string_lossy().as_ref()).unwrap();
        assert!(scanned.iter().any(|s| s.name == "second-skill"));

        // to_prompt includes the enabled skill bodies.
        let prompt = to_prompt(&ctx).unwrap();
        assert!(prompt.contains("test-skill"));
        assert!(prompt.contains("Test skill body"));

        // Per-name cache clear is a no-op error-wise and preserves the loader.
        clear_cache_by_name(&ctx, "test-skill").unwrap();
        assert!(has_skill(&ctx, "test-skill"));

        // reload clears caches and rescans the root.
        let reloaded = reload(&ctx, dir.to_string_lossy().as_ref()).unwrap();
        assert!(reloaded.iter().any(|s| s.name == "second-skill"));

        std::fs::remove_dir_all(&dir).ok();
        cleanup("scan");
    }
}
