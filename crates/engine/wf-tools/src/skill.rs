//! Skill loading: directory scanning, enable/disable state and content
//! caching.
//!
//! Parsing lives in [`parse`], metadata prompt generation in [`prompt`],
//! and resource file conventions in [`resources`].

pub mod parse;
pub mod prompt;
pub mod resources;

#[cfg(test)]
mod tests;

pub use parse::substitute_variables;
pub use prompt::{
    generate_skill_metadata_prompt, inject_skill_metadata, SKILLS_METADATA_PLACEHOLDER,
};
pub use resources::ResourceContent;

use dashmap::DashMap;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use std::time::{Duration, Instant};
use wf_common::lock::write_ok;

use parse::{parse_skill_md, strip_frontmatter};
use prompt::collect_relative_files;
use resources::{resource_type_dir_name, RESOURCE_DIRS};

use crate::error::{ToolError, ToolResult};
use crate::skill_fs::{HostSkillLoader, SkillFileLoader};
use wf_types::skill::{Skill, SkillConfig, SkillMetadata, SkillResourceType};

const CACHE_TTL: Duration = Duration::from_secs(300);
const CONTENT_CACHE_MAX: usize = 100;
const RESOURCE_CACHE_MAX: usize = 500;

/// Optional context for loading skill content:
/// variable substitution and permission validation.
#[derive(Debug, Clone, Default)]
pub struct SkillLoadContext {
    /// `{{name}}` placeholders replaced with the given values.
    pub variables: Option<HashMap<String, Value>>,
    /// Tools available at the call site; when set and the skill declares
    /// `allowedTools`, loading fails if any declared tool is unavailable.
    pub tools: Option<Vec<String>>,
}

/// Events emitted by the skill loader
/// (load started/completed/failed, enable/disable, cache clear).
#[derive(Debug, Clone, PartialEq)]
pub enum SkillEvent {
    LoadStarted { name: String },
    LoadCompleted { name: String },
    LoadFailed { name: String, error: String },
    Enabled { name: String },
    Disabled { name: String },
    CacheCleared { name: Option<String> },
}

/// Callback invoked for every skill loader event.
pub type SkillEventHandler = Arc<dyn Fn(&SkillEvent) + Send + Sync>;

pub(super) struct CacheEntry {
    content: ResourceContent,
    timestamp: Instant,
}

struct SkillEntry {
    metadata: SkillMetadata,
    path: PathBuf,
    resources: HashMap<SkillResourceType, Vec<String>>,
}

pub struct SkillLoader {
    skills: DashMap<String, SkillEntry>,
    enabled: RwLock<HashSet<String>>,
    content_cache: Mutex<HashMap<String, CacheEntry>>,
    resource_cache: Mutex<HashMap<String, CacheEntry>>,
    event_handler: RwLock<Option<SkillEventHandler>>,
    loader: Arc<dyn SkillFileLoader>,
}

impl SkillLoader {
    /// Create a loader over the host filesystem.
    pub fn new(config: SkillConfig) -> Self {
        Self::with_loader(config, Arc::new(HostSkillLoader))
    }

    /// Create a loader with a custom file system backend (tests, VFS).
    /// The loader must be supplied at construction time: scanning runs
    /// during construction (`with_config`), so a late swap would leave the
    /// scanned state inconsistent with the loader's source.
    pub fn with_loader(config: SkillConfig, loader: Arc<dyn SkillFileLoader>) -> Self {
        Self {
            skills: DashMap::new(),
            enabled: RwLock::new(HashSet::new()),
            content_cache: Mutex::new(HashMap::new()),
            resource_cache: Mutex::new(HashMap::new()),
            event_handler: RwLock::new(None),
            loader,
        }
        .with_config(config)
    }

    /// Install an event handler receiving skill loader events (load
    /// started/completed/failed, enable/disable, cache clear).
    pub fn set_event_handler(&self, handler: SkillEventHandler) {
        *wf_common::lock::write_ok(self.event_handler.write()) = Some(handler);
    }

    fn emit(&self, event: SkillEvent) {
        let handler = wf_common::lock::read_ok(self.event_handler.read()).clone();
        if let Some(handler) = handler {
            handler(&event);
        }
    }

    fn with_config(self, config: SkillConfig) -> Self {
        if config.auto_scan.unwrap_or(true) {
            for path in &config.paths {
                self.scan_path(Path::new(path));
            }
        }
        self
    }

    /// Scan a skills root directory: every subdirectory containing SKILL.md is a skill.
    pub fn scan_path(&self, skills_path: &Path) {
        let entries = match self.loader.read_dir(skills_path) {
            Ok(entries) => entries,
            Err(_) => return,
        };

        for entry in entries {
            if !entry.is_dir {
                continue;
            }
            let path = skills_path.join(&entry.name);
            let skill_md = path.join("SKILL.md");
            if self.loader.exists(&skill_md) {
                let _ = self.load_skill_dir(&path);
            }
        }
    }

    /// Load a skill from its directory, returning an error if parsing or validation fails.
    pub fn load_skill_dir(&self, skill_dir: &Path) -> ToolResult<Skill> {
        let skill_md_path = skill_dir.join("SKILL.md");
        let content = self.loader.read_text(&skill_md_path).map_err(|e| {
            ToolError::ExecutionError(format!(
                "Failed to read SKILL.md at {}: {}",
                skill_md_path.display(),
                e
            ))
        })?;

        let metadata = parse_skill_md(&content, skill_dir)?;

        // Verify that the directory name matches the name field.
        let dir_name = skill_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if metadata.name != dir_name {
            return Err(ToolError::ValidationFailed(format!(
                "Skill directory name '{}' does not match skill name '{}'",
                dir_name, metadata.name
            )));
        }

        // Auto-discover resource directories and populate resource file names.
        let mut resources: HashMap<SkillResourceType, Vec<String>> = HashMap::new();
        for resource_type in RESOURCE_DIRS {
            let dir = skill_dir.join(resource_type_dir_name(&resource_type));
            let mut files = Vec::new();
            collect_relative_files(self.loader.as_ref(), &dir, &dir, &mut files);
            if !files.is_empty() {
                resources.insert(resource_type, files);
            }
        }

        let skill = Skill {
            metadata: metadata.clone(),
            path: skill_dir.to_string_lossy().to_string(),
            content: None,
            references: resources
                .contains_key(&SkillResourceType::References)
                .then(HashMap::new),
            examples: resources
                .contains_key(&SkillResourceType::Examples)
                .then(HashMap::new),
            scripts: resources
                .contains_key(&SkillResourceType::Scripts)
                .then(HashMap::new),
            assets: resources
                .contains_key(&SkillResourceType::Assets)
                .then(HashMap::new),
        };

        self.skills.insert(
            metadata.name.clone(),
            SkillEntry {
                metadata,
                path: skill_dir.to_path_buf(),
                resources,
            },
        );
        // Newly loaded skills are enabled by default.
        write_ok(self.enabled.write()).insert(skill.metadata.name.clone());

        Ok(skill)
    }

    pub fn list_skills(&self) -> Vec<SkillMetadata> {
        self.skills
            .iter()
            .map(|e| e.value().metadata.clone())
            .collect()
    }

    pub fn has_skill(&self, name: &str) -> bool {
        self.skills.contains_key(name)
    }

    pub fn get_skill(&self, name: &str) -> Option<SkillMetadata> {
        self.skills.get(name).map(|e| e.value().metadata.clone())
    }

    /// Enable a skill by name; errors when the skill does not exist.
    pub fn enable_skill(&self, name: &str) -> ToolResult<()> {
        if !self.skills.contains_key(name) {
            return Err(ToolError::NotFound(format!("Skill '{}' not found", name)));
        }
        wf_common::lock::write_ok(self.enabled.write()).insert(name.to_string());
        self.emit(SkillEvent::Enabled {
            name: name.to_string(),
        });
        Ok(())
    }

    /// Disable a skill by name; errors when the skill does not exist.
    pub fn disable_skill(&self, name: &str) -> ToolResult<()> {
        if !self.skills.contains_key(name) {
            return Err(ToolError::NotFound(format!("Skill '{}' not found", name)));
        }
        wf_common::lock::write_ok(self.enabled.write()).remove(name);
        self.emit(SkillEvent::Disabled {
            name: name.to_string(),
        });
        Ok(())
    }

    /// Whether the skill exists and is enabled.
    pub fn is_skill_enabled(&self, name: &str) -> bool {
        self.skills.contains_key(name)
            && wf_common::lock::read_ok(self.enabled.read()).contains(name)
    }

    pub fn get_enabled_skills(&self) -> Vec<SkillMetadata> {
        let enabled = wf_common::lock::read_ok(self.enabled.read());
        self.skills
            .iter()
            .filter(|e| enabled.contains(e.key()))
            .map(|e| e.value().metadata.clone())
            .collect()
    }

    pub fn get_disabled_skills(&self) -> Vec<SkillMetadata> {
        let enabled = wf_common::lock::read_ok(self.enabled.read());
        self.skills
            .iter()
            .filter(|e| !enabled.contains(e.key()))
            .map(|e| e.value().metadata.clone())
            .collect()
    }

    /// Load the full body content of a skill (SKILL.md with frontmatter
    /// stripped). Equivalent to `load_skill_content` without context.
    pub fn load_content(&self, name: &str) -> ToolResult<String> {
        self.load_skill_content(name, None)
    }

    /// Load skill content with optional context: verifies the skill is
    /// enabled, validates `allowedTools` permissions when the context lists
    /// available tools, and substitutes `{{variable}}` placeholders.
    pub fn load_skill_content(
        &self,
        name: &str,
        context: Option<&SkillLoadContext>,
    ) -> ToolResult<String> {
        self.emit(SkillEvent::LoadStarted {
            name: name.to_string(),
        });

        let entry = self
            .skills
            .get(name)
            .ok_or_else(|| ToolError::NotFound(format!("Skill '{}' not found", name)))?;

        if !self.is_skill_enabled(name) {
            let error = ToolError::ExecutionError(format!("Skill '{}' is disabled", name));
            self.emit(SkillEvent::LoadFailed {
                name: name.to_string(),
                error: error.to_string(),
            });
            return Err(error);
        }

        if let Some(context) = context {
            if let Some(tools) = &context.tools {
                if let Some(allowed) = &entry.value().metadata.allowed_tools {
                    let missing: Vec<&String> =
                        allowed.iter().filter(|t| !tools.contains(t)).collect();
                    if !missing.is_empty() {
                        let error = ToolError::ValidationFailed(format!(
                            "Skill '{}' requires tools that are not allowed: {}",
                            name,
                            missing
                                .iter()
                                .map(|t| t.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ));
                        self.emit(SkillEvent::LoadFailed {
                            name: name.to_string(),
                            error: error.to_string(),
                        });
                        return Err(error);
                    }
                }
            }
        }

        let base = match self.load_content_base(name) {
            Ok(base) => base,
            Err(error) => {
                self.emit(SkillEvent::LoadFailed {
                    name: name.to_string(),
                    error: error.to_string(),
                });
                return Err(error);
            }
        };

        let content = match context.and_then(|c| c.variables.as_ref()) {
            Some(variables) if !variables.is_empty() => substitute_variables(&base, variables),
            _ => base,
        };

        self.emit(SkillEvent::LoadCompleted {
            name: name.to_string(),
        });
        Ok(content)
    }

    /// Load the raw content body, bypassing the variable/permission layers
    /// (used internally and by the progressive-disclosure prompt path).
    fn load_content_base(&self, name: &str) -> ToolResult<String> {
        let entry = self
            .skills
            .get(name)
            .ok_or_else(|| ToolError::NotFound(format!("Skill '{}' not found", name)))?;

        {
            let cache = wf_common::lock::lock_ok(self.content_cache.lock());
            if let Some(cached) = cache.get(name) {
                if cached.timestamp.elapsed() < CACHE_TTL {
                    if let ResourceContent::Text(text) = &cached.content {
                        return Ok(text.clone());
                    }
                }
            }
        }

        let path = entry.value().path.join("SKILL.md");
        let content = self.loader.read_text(&path).map_err(|e| {
            ToolError::ExecutionError(format!("Failed to read {}: {}", path.display(), e))
        })?;

        let body = strip_frontmatter(&content);

        let mut cache = wf_common::lock::lock_ok(self.content_cache.lock());
        clear_expired(&mut cache);
        cache.insert(
            name.to_string(),
            CacheEntry {
                content: ResourceContent::Text(body.clone()),
                timestamp: Instant::now(),
            },
        );
        evict_oldest(&mut cache, CONTENT_CACHE_MAX);

        Ok(body)
    }

    pub fn clear_cache(&self) {
        wf_common::lock::lock_ok(self.content_cache.lock()).clear();
        wf_common::lock::lock_ok(self.resource_cache.lock()).clear();
        self.emit(SkillEvent::CacheCleared { name: None });
    }

    /// Clear the cached content/resource entries of one skill only.
    pub fn clear_cache_for(&self, name: &str) {
        wf_common::lock::lock_ok(self.content_cache.lock()).remove(name);
        wf_common::lock::lock_ok(self.resource_cache.lock()).remove(name);
        self.emit(SkillEvent::CacheCleared {
            name: Some(name.to_string()),
        });
    }
}

fn clear_expired(cache: &mut HashMap<String, CacheEntry>) {
    cache.retain(|_, entry| entry.timestamp.elapsed() < CACHE_TTL);
}

fn evict_oldest(cache: &mut HashMap<String, CacheEntry>, max_size: usize) {
    while cache.len() >= max_size {
        let Some(oldest_key) = cache
            .iter()
            .min_by_key(|(_, entry)| entry.timestamp)
            .map(|(k, _)| k.clone())
        else {
            break;
        };
        cache.remove(&oldest_key);
    }
}

pub type SkillResourceContent = ResourceContent;
