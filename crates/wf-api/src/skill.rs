use std::sync::Arc;

use serde::Serialize;

use wf_tools::SkillLoader;
use wf_types::skill::{SkillMetadata, SkillResourceType};

use crate::context::ApiContext;
use crate::error::{ApiError, ApiResult};

/// Skill filter (mirrors the TS `SkillFilter`).
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

/// Skill resource management (TS `SkillRegistryAPI` counterpart).
///
/// Backed by the shared `ToolRegistry` skill loader; when no loader is wired
/// into the context (no skill paths configured), the metadata queries degrade
/// to empty results instead of erroring.
pub struct SkillApi {
    ctx: Arc<ApiContext>,
}

impl SkillApi {
    pub fn new(ctx: Arc<ApiContext>) -> Self {
        Self { ctx }
    }

    /// Resolve the shared skill loader; errors with a descriptive message
    /// when no loader is configured.
    fn loader(&self) -> ApiResult<Arc<SkillLoader>> {
        self.ctx.tool_registry.skill_loader().ok_or_else(|| {
            ApiError::Execution(
                "Skill system is not available: no skill loader is configured".into(),
            )
        })
    }

    /// Whether the skill loader is present in this context.
    pub fn is_available(&self) -> bool {
        self.ctx.tool_registry.skill_loader().is_some()
    }

    // ── metadata queries ────────────────────────────────────────────

    /// Metadata of all known skills (empty when no loader is configured).
    pub fn list_skills(&self) -> ApiResult<Vec<SkillMetadata>> {
        match self.ctx.tool_registry.skill_loader() {
            Some(loader) => Ok(loader.list_skills()),
            None => Ok(Vec::new()),
        }
    }

    pub fn get_skill(&self, name: &str) -> ApiResult<SkillMetadata> {
        let loader = self.loader()?;
        loader
            .get_skill(name)
            .ok_or_else(|| ApiError::not_found("skill", name))
    }

    pub fn has_skill(&self, name: &str) -> bool {
        self.ctx
            .tool_registry
            .skill_loader()
            .map(|loader| loader.has_skill(name))
            .unwrap_or(false)
    }

    pub fn get_enabled_skills(&self) -> ApiResult<Vec<SkillMetadata>> {
        let loader = self.loader()?;
        Ok(loader.get_enabled_skills())
    }

    pub fn get_disabled_skills(&self) -> ApiResult<Vec<SkillMetadata>> {
        let loader = self.loader()?;
        Ok(loader.get_disabled_skills())
    }

    /// Filter skills by name / version / tags.
    pub fn query(&self, filter: &SkillFilter) -> ApiResult<Vec<SkillMetadata>> {
        Ok(self
            .list_skills()?
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

    pub fn enable(&self, name: &str) -> ApiResult<()> {
        let loader = self.loader()?;
        loader.enable_skill(name)?;
        Ok(())
    }

    pub fn disable(&self, name: &str) -> ApiResult<()> {
        let loader = self.loader()?;
        loader.disable_skill(name)?;
        Ok(())
    }

    pub fn is_enabled(&self, name: &str) -> ApiResult<bool> {
        let loader = self.loader()?;
        Ok(loader.is_skill_enabled(name))
    }

    // ── cache ───────────────────────────────────────────────────────

    /// Clear the content/resource caches of the skill loader.
    pub fn clear_cache(&self) -> ApiResult<()> {
        let loader = self.loader()?;
        loader.clear_cache();
        Ok(())
    }

    // ── progressive disclosure ──────────────────────────────────────

    /// Level 1: metadata prompt listing the enabled skills.
    pub fn generate_metadata_prompt(&self) -> ApiResult<String> {
        let loader = self.loader()?;
        Ok(wf_tools::skill::generate_skill_metadata_prompt(
            &loader.get_enabled_skills(),
        ))
    }

    /// Level 1: inject the skill metadata prompt into a system prompt.
    pub fn inject_skill_metadata(&self, system_prompt: &str) -> ApiResult<String> {
        let loader = self.loader()?;
        Ok(wf_tools::skill::inject_skill_metadata(
            system_prompt,
            &loader.get_enabled_skills(),
        ))
    }

    /// Level 2: load the full skill body content.
    pub fn load_content(&self, name: &str) -> ApiResult<String> {
        let loader = self.loader()?;
        loader.load_content(name).map_err(Into::into)
    }

    /// Level 3: list the relative paths of a skill resource directory.
    pub fn list_resources(
        &self,
        name: &str,
        resource_type: SkillResourceType,
    ) -> ApiResult<Vec<String>> {
        let loader = self.loader()?;
        Ok(loader.list_skill_resources(name, resource_type))
    }

    /// Level 3: load one resource file of a skill.
    pub fn load_resource(
        &self,
        name: &str,
        resource_type: SkillResourceType,
        path: &str,
    ) -> ApiResult<String> {
        let loader = self.loader()?;
        match loader.load_skill_resource(name, resource_type, path)? {
            wf_tools::skill::ResourceContent::Text(text) => Ok(text),
            wf_tools::skill::ResourceContent::Binary(bytes) => {
                Ok(String::from_utf8_lossy(&bytes).into_owned())
            }
        }
    }

    /// Level 3: load all resources of a skill directory.
    pub fn load_resources(
        &self,
        name: &str,
        resource_type: SkillResourceType,
    ) -> ApiResult<Vec<SkillResourceEntry>> {
        let loader = self.loader()?;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use wf_resource::registrar::Registries;
    use wf_resource::starter::BundleRegistry;
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
            Arc::new(Registries::new()),
            Arc::new(BundleRegistry::new()),
        );
        ctx.tool_registry.set_skill_loader(loader);
        Arc::new(ctx)
    }

    fn cleanup(tag: &str) {
        let dir = std::env::temp_dir().join(format!("wf-api-skill-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn skill_metadata_query_and_filter() {
        let ctx = make_ctx("meta");
        let api = SkillApi::new(ctx);

        let all = api.list_skills().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "test-skill");
        assert!(api.has_skill("test-skill"));

        let matched = api
            .query(&SkillFilter {
                name: Some("test".into()),
                ..SkillFilter::default()
            })
            .unwrap();
        assert_eq!(matched.len(), 1);

        let err = api.get_skill("nope").unwrap_err();
        assert!(matches!(err, ApiError::NotFound { .. }));

        cleanup("meta");
    }

    #[test]
    fn skill_enable_disable() {
        let ctx = make_ctx("enable");
        let api = SkillApi::new(ctx);

        assert!(api.is_enabled("test-skill").unwrap());
        api.disable("test-skill").unwrap();
        assert!(!api.is_enabled("test-skill").unwrap());
        assert_eq!(api.get_disabled_skills().unwrap().len(), 1);
        assert!(api.get_enabled_skills().unwrap().is_empty());
        api.enable("test-skill").unwrap();
        assert!(api.is_enabled("test-skill").unwrap());

        let err = api.enable("missing-skill").unwrap_err();
        assert!(matches!(err, ApiError::Execution(_)));

        cleanup("enable");
    }

    #[test]
    fn skill_progressive_disclosure() {
        let ctx = make_ctx("disclosure");
        let api = SkillApi::new(ctx);

        let prompt = api.generate_metadata_prompt().unwrap();
        assert!(prompt.contains("test-skill"));

        let injected = api.inject_skill_metadata("You are a helper.").unwrap();
        assert!(injected.contains("test-skill"));

        let content = api.load_content("test-skill").unwrap();
        assert!(content.contains("Test skill body"));

        let resources = api
            .list_resources("test-skill", SkillResourceType::References)
            .unwrap();
        assert!(resources.iter().any(|p| p.ends_with("guide.md")));

        let guide = api
            .load_resource("test-skill", SkillResourceType::References, "guide.md")
            .unwrap();
        assert_eq!(guide, "reference content");

        let all = api
            .load_resources("test-skill", SkillResourceType::References)
            .unwrap();
        assert_eq!(all.len(), 1);

        cleanup("disclosure");
    }

    #[test]
    fn unavailable_loader_degrades() {
        let ctx = Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(Registries::new()),
            Arc::new(BundleRegistry::new()),
        ));
        let api = SkillApi::new(ctx);
        assert!(!api.is_available());
        assert!(api.list_skills().unwrap().is_empty());
        let err = api.get_skill("x").unwrap_err();
        assert!(matches!(err, ApiError::Execution(_)));
        let _ = PathBuf::new();
    }
}
