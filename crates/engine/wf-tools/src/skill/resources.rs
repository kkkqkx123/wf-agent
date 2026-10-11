//! Skill resource files: directory conventions, content loading and listing.
//!
//! Resource directories live under the skill directory and are auto-discovered
//! at scan time; the same layout is used for loading by relative path.

use std::collections::HashMap;
use std::time::Instant;

use wf_types::skill::SkillResourceType;

use super::{clear_expired, evict_oldest, CacheEntry, SkillLoader, CACHE_TTL, RESOURCE_CACHE_MAX};
use crate::error::{ToolError, ToolResult};

/// Loaded content of a skill resource file: text (references/examples/scripts) or binary (assets).
#[derive(Debug, Clone)]
pub enum ResourceContent {
    Text(String),
    Binary(Vec<u8>),
}

pub(super) const RESOURCE_DIRS: [SkillResourceType; 4] = [
    SkillResourceType::References,
    SkillResourceType::Examples,
    SkillResourceType::Scripts,
    SkillResourceType::Assets,
];

pub(super) fn resource_type_dir_name(resource_type: &SkillResourceType) -> &'static str {
    match resource_type {
        SkillResourceType::References => "references",
        SkillResourceType::Examples => "examples",
        SkillResourceType::Scripts => "scripts",
        SkillResourceType::Assets => "assets",
    }
}

impl SkillLoader {
    /// Load a skill resource file by relative path within its resource directory.
    pub fn load_skill_resource(
        &self,
        name: &str,
        resource_type: SkillResourceType,
        resource_path: &str,
    ) -> ToolResult<ResourceContent> {
        let entry = self
            .skills
            .get(name)
            .ok_or_else(|| ToolError::NotFound(format!("Skill '{}' not found", name)))?;

        let cache_key = format!("{}:{:?}:{}", name, resource_type, resource_path);

        {
            let cache = wf_common::lock::lock_ok(self.resource_cache.lock());
            if let Some(cached) = cache.get(&cache_key) {
                if cached.timestamp.elapsed() < CACHE_TTL {
                    return Ok(cached.content.clone());
                }
            }
        }

        let resource_dir = entry
            .value()
            .path
            .join(resource_type_dir_name(&resource_type));
        let full_path = resource_dir.join(resource_path);

        // Prevent path traversal outside the resource directory.
        let canonical_dir = self.loader.canonicalize(&resource_dir).map_err(|e| {
            ToolError::ExecutionError(format!(
                "Failed to resolve resource dir {}: {}",
                resource_dir.display(),
                e
            ))
        })?;
        let canonical_path = self.loader.canonicalize(&full_path).map_err(|e| {
            ToolError::ExecutionError(format!(
                "Resource '{}' not found for skill '{}': {}",
                resource_path, name, e
            ))
        })?;
        if !canonical_path.starts_with(&canonical_dir) {
            return Err(ToolError::ValidationFailed(format!(
                "Resource path '{}' escapes skill resource directory",
                resource_path
            )));
        }

        let content = if resource_type == SkillResourceType::Assets {
            let bytes = self.loader.read_binary(&canonical_path).map_err(|e| {
                ToolError::ExecutionError(format!("Failed to read resource: {}", e))
            })?;
            ResourceContent::Binary(bytes)
        } else {
            let text = self.loader.read_text(&canonical_path).map_err(|e| {
                ToolError::ExecutionError(format!("Failed to read resource: {}", e))
            })?;
            ResourceContent::Text(text)
        };

        let mut cache = wf_common::lock::lock_ok(self.resource_cache.lock());
        clear_expired(&mut cache);
        cache.insert(
            cache_key,
            CacheEntry {
                content: content.clone(),
                timestamp: Instant::now(),
            },
        );
        evict_oldest(&mut cache, RESOURCE_CACHE_MAX);

        Ok(content)
    }

    /// List all resource file paths (relative to the resource directory) of a skill.
    pub fn list_skill_resources(
        &self,
        name: &str,
        resource_type: SkillResourceType,
    ) -> Vec<String> {
        self.skills
            .get(name)
            .map(|e| {
                e.value()
                    .resources
                    .get(&resource_type)
                    .cloned()
                    .unwrap_or_default()
            })
            .unwrap_or_default()
    }

    /// Load all resources of a given type into the returned map (key: relative path).
    pub fn load_resources(
        &self,
        name: &str,
        resource_type: SkillResourceType,
    ) -> ToolResult<HashMap<String, ResourceContent>> {
        let paths = self.list_skill_resources(name, resource_type.clone());
        let mut result = HashMap::new();
        for path in paths {
            let content = self.load_skill_resource(name, resource_type.clone(), &path)?;
            result.insert(path, content);
        }
        Ok(result)
    }
}
