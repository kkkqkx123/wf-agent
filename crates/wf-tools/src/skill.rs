use dashmap::DashMap;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};
use std::time::{Duration, Instant};

use crate::error::{ToolError, ToolResult};
use wf_types::skill::{Skill, SkillConfig, SkillMetadata, SkillResourceType};

const CACHE_TTL: Duration = Duration::from_secs(300);
const CONTENT_CACHE_MAX: usize = 100;
const RESOURCE_CACHE_MAX: usize = 500;

const RESOURCE_DIRS: [SkillResourceType; 4] = [
    SkillResourceType::References,
    SkillResourceType::Examples,
    SkillResourceType::Scripts,
    SkillResourceType::Assets,
];

/// Loaded content of a skill resource file: text (references/examples/scripts) or binary (assets).
#[derive(Debug, Clone)]
pub enum ResourceContent {
    Text(String),
    Binary(Vec<u8>),
}

/// Optional context for loading skill content (mirrors the TS
/// `SkillLoadContext`): variable substitution and permission validation.
#[derive(Debug, Clone, Default)]
pub struct SkillLoadContext {
    /// `{{name}}` placeholders replaced with the given values.
    pub variables: Option<HashMap<String, Value>>,
    /// Tools available at the call site; when set and the skill declares
    /// `allowedTools`, loading fails if any declared tool is unavailable.
    pub tools: Option<Vec<String>>,
}

struct CacheEntry {
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
}

impl SkillLoader {
    pub fn new(config: SkillConfig) -> Self {
        Self {
            skills: DashMap::new(),
            enabled: RwLock::new(HashSet::new()),
            content_cache: Mutex::new(HashMap::new()),
            resource_cache: Mutex::new(HashMap::new()),
        }
        .with_config(config)
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
        let entries = match std::fs::read_dir(skills_path) {
            Ok(entries) => entries,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let skill_md = path.join("SKILL.md");
            if skill_md.exists() {
                let _ = self.load_skill_dir(&path);
            }
        }
    }

    /// Load a skill from its directory, returning an error if parsing or validation fails.
    pub fn load_skill_dir(&self, skill_dir: &Path) -> ToolResult<Skill> {
        let skill_md_path = skill_dir.join("SKILL.md");
        let content = std::fs::read_to_string(&skill_md_path).map_err(|e| {
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
            collect_relative_files(&dir, &dir, &mut files);
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
        self.enabled
            .write()
            .unwrap()
            .insert(skill.metadata.name.clone());

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
        self.enabled.write().unwrap().insert(name.to_string());
        Ok(())
    }

    /// Disable a skill by name; errors when the skill does not exist.
    pub fn disable_skill(&self, name: &str) -> ToolResult<()> {
        if !self.skills.contains_key(name) {
            return Err(ToolError::NotFound(format!("Skill '{}' not found", name)));
        }
        self.enabled.write().unwrap().remove(name);
        Ok(())
    }

    /// Whether the skill exists and is enabled.
    pub fn is_skill_enabled(&self, name: &str) -> bool {
        self.skills.contains_key(name) && self.enabled.read().unwrap().contains(name)
    }

    pub fn get_enabled_skills(&self) -> Vec<SkillMetadata> {
        let enabled = self.enabled.read().unwrap();
        self.skills
            .iter()
            .filter(|e| enabled.contains(e.key()))
            .map(|e| e.value().metadata.clone())
            .collect()
    }

    pub fn get_disabled_skills(&self) -> Vec<SkillMetadata> {
        let enabled = self.enabled.read().unwrap();
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
        let entry = self
            .skills
            .get(name)
            .ok_or_else(|| ToolError::NotFound(format!("Skill '{}' not found", name)))?;

        if !self.is_skill_enabled(name) {
            return Err(ToolError::ExecutionError(format!(
                "Skill '{}' is disabled",
                name
            )));
        }

        if let Some(context) = context {
            if let Some(tools) = &context.tools {
                if let Some(allowed) = &entry.value().metadata.allowed_tools {
                    let missing: Vec<&String> =
                        allowed.iter().filter(|t| !tools.contains(t)).collect();
                    if !missing.is_empty() {
                        return Err(ToolError::ValidationFailed(format!(
                            "Skill '{}' requires tools that are not allowed: {}",
                            name,
                            missing
                                .iter()
                                .map(|t| t.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )));
                    }
                }
            }
        }

        let base = self.load_content_base(name)?;

        let content = match context.and_then(|c| c.variables.as_ref()) {
            Some(variables) if !variables.is_empty() => substitute_variables(&base, variables),
            _ => base,
        };

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
            let cache = self.content_cache.lock().unwrap();
            if let Some(cached) = cache.get(name) {
                if cached.timestamp.elapsed() < CACHE_TTL {
                    if let ResourceContent::Text(text) = &cached.content {
                        return Ok(text.clone());
                    }
                }
            }
        }

        let path = entry.value().path.join("SKILL.md");
        let content = std::fs::read_to_string(&path).map_err(|e| {
            ToolError::ExecutionError(format!("Failed to read {}: {}", path.display(), e))
        })?;

        let body = strip_frontmatter(&content);

        let mut cache = self.content_cache.lock().unwrap();
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
            let cache = self.resource_cache.lock().unwrap();
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
        let canonical_dir = resource_dir.canonicalize().map_err(|e| {
            ToolError::ExecutionError(format!(
                "Failed to resolve resource dir {}: {}",
                resource_dir.display(),
                e
            ))
        })?;
        let canonical_path = full_path.canonicalize().map_err(|e| {
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
            let bytes = std::fs::read(&canonical_path).map_err(|e| {
                ToolError::ExecutionError(format!("Failed to read resource: {}", e))
            })?;
            ResourceContent::Binary(bytes)
        } else {
            let text = std::fs::read_to_string(&canonical_path).map_err(|e| {
                ToolError::ExecutionError(format!("Failed to read resource: {}", e))
            })?;
            ResourceContent::Text(text)
        };

        let mut cache = self.resource_cache.lock().unwrap();
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

    pub fn clear_cache(&self) {
        self.content_cache.lock().unwrap().clear();
        self.resource_cache.lock().unwrap().clear();
    }
}

fn resource_type_dir_name(resource_type: &SkillResourceType) -> &'static str {
    match resource_type {
        SkillResourceType::References => "references",
        SkillResourceType::Examples => "examples",
        SkillResourceType::Scripts => "scripts",
        SkillResourceType::Assets => "assets",
    }
}

/// Replace `{{name}}` placeholders with the given values. Non-string values
/// are rendered via their JSON representation (null/absent → empty string).
pub fn substitute_variables(content: &str, variables: &HashMap<String, Value>) -> String {
    let mut result = content.to_string();
    for (key, value) in variables {
        let placeholder = format!("{{{{{}}}}}", key);
        let replacement = match value {
            Value::Null => String::new(),
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        result = result.replace(&placeholder, &replacement);
    }
    result
}

/// Placeholder replaced by [`inject_skill_metadata`], mirroring the TS
/// `{SKILLS_METADATA}` token.
pub const SKILLS_METADATA_PLACEHOLDER: &str = "{SKILLS_METADATA}";

/// Generate the metadata prompt listing all enabled skills
/// (progressive disclosure level 1).
pub fn generate_skill_metadata_prompt(skills: &[SkillMetadata]) -> String {
    if skills.is_empty() {
        return String::new();
    }
    let mut lines = vec!["Available skills:".to_string()];
    for skill in skills {
        let mut line = format!("  - {}: {}", skill.name, skill.description);
        if let Some(version) = &skill.version {
            line.push_str(&format!(" (v{})", version));
        }
        lines.push(line);
    }
    lines.join("\n")
}

/// Inject the skill metadata prompt into a system prompt: replaces the
/// `{SKILLS_METADATA}` placeholder when present, otherwise appends the
/// metadata at the end. Returns the (possibly unchanged) prompt.
pub fn inject_skill_metadata(
    system_prompt: &str,
    enabled_skills: &[SkillMetadata],
) -> String {
    let metadata_prompt = generate_skill_metadata_prompt(enabled_skills);

    if metadata_prompt.is_empty() {
        return system_prompt.replace(SKILLS_METADATA_PLACEHOLDER, "");
    }

    if system_prompt.contains(SKILLS_METADATA_PLACEHOLDER) {
        return system_prompt.replace(SKILLS_METADATA_PLACEHOLDER, &metadata_prompt);
    }

    if enabled_skills.is_empty() {
        return system_prompt.to_string();
    }

    format!("{}\n\n{}", system_prompt, metadata_prompt)
}

fn collect_relative_files(dir: &Path, root: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_relative_files(&path, root, out);
        } else if path.is_file() {
            if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().to_string());
            }
        }
    }
}

fn strip_frontmatter(content: &str) -> String {
    let body_match = content.strip_prefix("---").and_then(|rest| {
        rest.find("\n---").map(|idx| {
            let start = idx + "\n---".len();
            rest[start..]
                .strip_prefix('\n')
                .unwrap_or(&rest[start..])
                .to_string()
        })
    });
    match body_match {
        Some(body) => body.trim().to_string(),
        None => content.trim().to_string(),
    }
}

fn parse_skill_md(content: &str, skill_dir: &Path) -> ToolResult<SkillMetadata> {
    let frontmatter = content
        .strip_prefix("---")
        .and_then(|rest| rest.find("\n---").map(|idx| &rest[..idx]))
        .ok_or_else(|| {
            ToolError::ValidationFailed(format!(
                "Missing YAML frontmatter in {}",
                skill_dir.display()
            ))
        })?;

    let fields = parse_yaml_frontmatter(frontmatter);

    let name = fields.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
        ToolError::ValidationFailed(format!(
            "Missing required field: name in {}",
            skill_dir.display()
        ))
    })?;

    if fields.get("description").and_then(|v| v.as_str()).is_none() {
        return Err(ToolError::ValidationFailed(format!(
            "Missing required field: description in {}",
            skill_dir.display()
        )));
    }

    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(ToolError::ValidationFailed(format!(
            "Invalid skill name '{}': must be lowercase alphanumeric with hyphens only",
            name
        )));
    }

    Ok(SkillMetadata {
        name: name.to_string(),
        description: fields
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        when_to_use: fields
            .get("when_to_use")
            .and_then(|v| v.as_str())
            .map(String::from),
        version: fields
            .get("version")
            .and_then(|v| v.as_str())
            .map(String::from),
        license: fields
            .get("license")
            .and_then(|v| v.as_str())
            .map(String::from),
        allowed_tools: fields
            .get("allowedTools")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            }),
        metadata: fields
            .get("metadata")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .map(|(k, v)| (k.clone(), v.to_string()))
                    .collect()
            }),
    })
}

/// Minimal line-based YAML frontmatter parsing (mirrors the TS implementation).
fn parse_yaml_frontmatter(yaml: &str) -> HashMap<String, Value> {
    let mut result: HashMap<String, Value> = HashMap::new();
    let mut in_array = false;
    let mut array_key: Option<String> = None;

    for line in yaml.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Some(item) = trimmed.strip_prefix("- ") {
            if in_array {
                if let Some(key) = &array_key {
                    let entry = result
                        .entry(key.clone())
                        .or_insert_with(|| Value::Array(vec![]));
                    if let Value::Array(arr) = entry {
                        arr.push(Value::String(item.trim().to_string()));
                    }
                }
            }
            continue;
        }

        let Some(colon_index) = trimmed.find(':') else {
            continue;
        };
        let key = trimmed[..colon_index].trim().to_string();
        let value = trimmed[colon_index + 1..].trim();

        if value.is_empty() || value == "[]" {
            array_key = Some(key.clone());
            in_array = true;
            if value == "[]" {
                result.insert(key, Value::Array(vec![]));
            }
        } else {
            result.insert(key, parse_yaml_value(value));
            in_array = false;
        }
    }

    result
}

fn parse_yaml_value(value: &str) -> Value {
    let value = value.trim();
    if (value.starts_with('"') && value.ends_with('"') && value.len() >= 2)
        || (value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2)
    {
        return Value::String(value[1..value.len() - 1].to_string());
    }

    match value {
        "true" => return Value::Bool(true),
        "false" => return Value::Bool(false),
        "null" | "~" => return Value::Null,
        _ => {}
    }

    if let Ok(num) = value.parse::<i64>() {
        return Value::Number(num.into());
    }
    if let Ok(num) = value.parse::<f64>() {
        if let Some(number) = serde_json::Number::from_f64(num) {
            return Value::Number(number);
        }
    }

    Value::String(value.to_string())
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

impl SkillResourceContent {
    #[allow(dead_code)]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            ResourceContent::Text(t) => Some(t),
            ResourceContent::Binary(_) => None,
        }
    }

    #[allow(dead_code)]
    pub fn as_binary(&self) -> Option<&[u8]> {
        match self {
            ResourceContent::Binary(b) => Some(b),
            ResourceContent::Text(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_skill_dir(root: &Path, name: &str) -> PathBuf {
        let dir = root.join(name);
        std::fs::create_dir_all(dir.join("references")).unwrap();
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        std::fs::write(
            dir.join("SKILL.md"),
            format!(
                "---\nname: {}\ndescription: Test skill\ndescription_extended: \"hello\"\nversion: 1.0.0\nallowedTools:\n- read_file\n- write_file\n---\n\n# Body\n\nContent here.",
                name
            ),
        )
        .unwrap();
        std::fs::write(dir.join("references").join("ref1.md"), "# Ref one").unwrap();
        std::fs::write(dir.join("scripts").join("run.py"), "print('hi')").unwrap();
        std::fs::write(dir.join("assets").join("logo.png"), vec![1u8, 2, 3, 4]).unwrap();
        dir
    }

    #[test]
    fn test_scan_and_list_skills() {
        let root = std::env::temp_dir().join(format!("wf-skill-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        make_skill_dir(&root, "my-skill");

        let loader = SkillLoader::new(SkillConfig {
            paths: vec![root.to_string_lossy().to_string()],
            auto_scan: Some(true),
        });

        let skills = loader.list_skills();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "my-skill");
        assert_eq!(skills[0].version.as_deref(), Some("1.0.0"));
        assert_eq!(
            skills[0].allowed_tools.as_deref(),
            Some(vec!["read_file".to_string(), "write_file".to_string()].as_slice())
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_name_mismatch_rejected() {
        let root = std::env::temp_dir().join(format!("wf-skill-mismatch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let dir = root.join("other-name");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("SKILL.md"),
            "---\nname: real-name\ndescription: x\n---\n\nBody",
        )
        .unwrap();

        let loader = SkillLoader::new(SkillConfig {
            paths: vec![root.to_string_lossy().to_string()],
            auto_scan: Some(true),
        });

        assert_eq!(loader.list_skills().len(), 0);
        assert!(loader.load_skill_dir(&dir).is_err());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_load_content_strips_frontmatter() {
        let root = std::env::temp_dir().join(format!("wf-skill-content-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        make_skill_dir(&root, "my-skill");

        let loader = SkillLoader::new(SkillConfig {
            paths: vec![root.to_string_lossy().to_string()],
            auto_scan: Some(true),
        });

        let content = loader.load_content("my-skill").unwrap();
        assert!(!content.contains("---"));
        assert!(content.contains("Content here"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_load_skill_resource() {
        let root = std::env::temp_dir().join(format!("wf-skill-res-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        make_skill_dir(&root, "my-skill");

        let loader = SkillLoader::new(SkillConfig {
            paths: vec![root.to_string_lossy().to_string()],
            auto_scan: Some(true),
        });

        let paths = loader.list_skill_resources("my-skill", SkillResourceType::References);
        assert_eq!(paths, vec!["ref1.md".to_string()]);

        let text = loader
            .load_skill_resource("my-skill", SkillResourceType::References, "ref1.md")
            .unwrap();
        assert_eq!(text.as_text(), Some("# Ref one"));

        let scripts = loader.load_skill_resource("my-skill", SkillResourceType::Scripts, "run.py");
        assert!(scripts.is_ok());

        let binary = loader
            .load_skill_resource("my-skill", SkillResourceType::Assets, "logo.png")
            .unwrap();
        assert_eq!(binary.as_binary(), Some(&[1u8, 2, 3, 4][..]));

        assert!(loader
            .load_skill_resource("my-skill", SkillResourceType::References, "missing.md")
            .is_err());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_path_traversal_rejected() {
        let root = std::env::temp_dir().join(format!("wf-skill-trav-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        make_skill_dir(&root, "my-skill");

        let loader = SkillLoader::new(SkillConfig {
            paths: vec![root.to_string_lossy().to_string()],
            auto_scan: Some(true),
        });

        assert!(loader
            .load_skill_resource("my-skill", SkillResourceType::References, "../SKILL.md")
            .is_err());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_load_resources_all() {
        let root = std::env::temp_dir().join(format!("wf-skill-all-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        make_skill_dir(&root, "my-skill");

        let loader = SkillLoader::new(SkillConfig {
            paths: vec![root.to_string_lossy().to_string()],
            auto_scan: Some(true),
        });

        let all = loader
            .load_resources("my-skill", SkillResourceType::Scripts)
            .unwrap();
        assert_eq!(all.len(), 1);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_yaml_parsing() {
        let fields = parse_yaml_frontmatter(
            "name: test\nwhen_to_use: \"quoted\"\nversion: 2\nflag: true\nnothing: null\n",
        );
        assert_eq!(fields.get("name").and_then(|v| v.as_str()), Some("test"));
        assert_eq!(fields.get("version").and_then(|v| v.as_i64()), Some(2));
        assert_eq!(fields.get("flag").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(fields.get("nothing"), Some(&Value::Null));
        assert_eq!(
            fields.get("when_to_use").and_then(|v| v.as_str()),
            Some("quoted")
        );
    }

    #[test]
    fn test_enable_disable_skill() {
        let root = std::env::temp_dir().join(format!("wf-skill-en-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        make_skill_dir(&root, "my-skill");

        let loader = SkillLoader::new(SkillConfig {
            paths: vec![root.to_string_lossy().to_string()],
            auto_scan: Some(true),
        });

        // Enabled by default.
        assert!(loader.is_skill_enabled("my-skill"));
        assert_eq!(loader.get_enabled_skills().len(), 1);
        assert!(loader.get_disabled_skills().is_empty());

        loader.disable_skill("my-skill").unwrap();
        assert!(!loader.is_skill_enabled("my-skill"));
        assert!(loader.load_content("my-skill").is_err());

        loader.enable_skill("my-skill").unwrap();
        assert!(loader.load_content("my-skill").is_ok());

        assert!(loader.enable_skill("missing").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_variable_substitution() {
        let root = std::env::temp_dir().join(format!("wf-skill-var-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("var-skill")).unwrap();
        std::fs::write(
            root.join("var-skill/SKILL.md"),
            "---\nname: var-skill\ndescription: Var\n---\n\nHello {{name}}, count={{count}} missing={{nope}}",
        )
        .unwrap();

        let loader = SkillLoader::new(SkillConfig {
            paths: vec![root.to_string_lossy().to_string()],
            auto_scan: Some(false),
        });
        let dir = root.join("var-skill");
        loader.load_skill_dir(&dir).unwrap();

        let mut variables = HashMap::new();
        variables.insert("name".to_string(), Value::String("world".into()));
        variables.insert("count".to_string(), Value::from(3));
        let context = SkillLoadContext {
            variables: Some(variables),
            tools: None,
        };
        let content = loader.load_skill_content("var-skill", Some(&context)).unwrap();
        assert!(content.contains("Hello world, count=3 missing="));

        // Without variables the placeholders remain.
        let content = loader.load_content("var-skill").unwrap();
        assert!(content.contains("{{name}}"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_allowed_tools_permission_check() {
        let root = std::env::temp_dir().join(format!("wf-skill-perm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("perm-skill")).unwrap();
        std::fs::write(
            root.join("perm-skill/SKILL.md"),
            "---\nname: perm-skill\ndescription: Perm\nallowedTools:\n- read_file\n- write_file\n---\n\nBody",
        )
        .unwrap();

        let loader = SkillLoader::new(SkillConfig {
            paths: vec![root.to_string_lossy().to_string()],
            auto_scan: Some(false),
        });
        let dir = root.join("perm-skill");
        loader.load_skill_dir(&dir).unwrap();

        // Missing tools → rejected.
        let denied = SkillLoadContext {
            variables: None,
            tools: Some(vec!["read_file".into()]),
        };
        assert!(loader.load_skill_content("perm-skill", Some(&denied)).is_err());

        // All tools present → allowed.
        let allowed = SkillLoadContext {
            variables: None,
            tools: Some(vec!["read_file".into(), "write_file".into()]),
        };
        assert!(loader.load_skill_content("perm-skill", Some(&allowed)).is_ok());

        // No context → permission skipped.
        assert!(loader.load_skill_content("perm-skill", None).is_ok());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_metadata_prompt_generation() {
        let skills = vec![
            SkillMetadata {
                name: "analyze-data".into(),
                description: "Analyze datasets".into(),
                when_to_use: None,
                version: Some("1.0.0".into()),
                license: None,
                allowed_tools: None,
                metadata: None,
            },
            SkillMetadata {
                name: "review".into(),
                description: "Review code".into(),
                when_to_use: None,
                version: None,
                license: None,
                allowed_tools: None,
                metadata: None,
            },
        ];

        let prompt = generate_skill_metadata_prompt(&skills);
        assert!(prompt.contains("Available skills:"));
        assert!(prompt.contains("analyze-data: Analyze datasets (v1.0.0)"));
        assert!(prompt.contains("review: Review code"));

        // Placeholder replacement.
        let injected = inject_skill_metadata(
            "You are a coder.\n{SKILLS_METADATA}",
            &skills,
        );
        assert!(!injected.contains("{SKILLS_METADATA}"));
        assert!(injected.contains("Available skills:"));

        // Append when no placeholder.
        let appended = inject_skill_metadata("You are a coder.", &skills);
        assert!(appended.contains("You are a coder."));
        assert!(appended.contains("Available skills:"));

        // No enabled skills → placeholder removed, prompt otherwise unchanged.
        let empty = inject_skill_metadata("Hi {SKILLS_METADATA}", &[]);
        assert_eq!(empty, "Hi ");
        assert!(inject_skill_metadata("Hi", &[]).contains("Hi"));
    }
}
