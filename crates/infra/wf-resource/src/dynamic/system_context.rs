use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use wf_types::tool_description::ToolDescriptionData;

use crate::predefined::render::{render_tool_descriptions, ToolFormat};

#[derive(Debug, Clone)]
pub struct SystemConfig {
    pub include_time: bool,
    pub include_env: bool,
    pub include_tool_descriptions: bool,
    pub include_skills: bool,
    pub include_workflows: bool,
    pub timezone: Option<String>,
    pub cache_ttl_ms: u64,
    pub tool_descriptions: Vec<ToolDescriptionData>,
    pub skills: Vec<String>,
    pub workflows: Vec<String>,
    pub custom_sections: Vec<(String, String)>,
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            include_time: true,
            include_env: true,
            include_tool_descriptions: false,
            include_skills: false,
            include_workflows: false,
            timezone: None,
            cache_ttl_ms: 60_000,
            tool_descriptions: Vec::new(),
            skills: Vec::new(),
            workflows: Vec::new(),
            custom_sections: Vec::new(),
        }
    }
}

struct CacheEntry {
    key: u64,
    value: String,
    expiry: i64,
}

/// Cache key over the FULL config (flags + dynamic payload). The previous
/// key hashed only the boolean flags, so different tool/skill/workflow
/// payloads collided onto one cached value.
fn cache_key(cfg: &SystemConfig) -> u64 {
    let mut hasher = DefaultHasher::new();
    cfg.include_time.hash(&mut hasher);
    cfg.include_env.hash(&mut hasher);
    cfg.include_tool_descriptions.hash(&mut hasher);
    cfg.include_skills.hash(&mut hasher);
    cfg.include_workflows.hash(&mut hasher);
    cfg.timezone.hash(&mut hasher);
    cfg.cache_ttl_ms.hash(&mut hasher);
    for tool in &cfg.tool_descriptions {
        format!("{:?}", tool).hash(&mut hasher);
    }
    for skill in &cfg.skills {
        skill.hash(&mut hasher);
    }
    for workflow in &cfg.workflows {
        workflow.hash(&mut hasher);
    }
    for (title, content) in &cfg.custom_sections {
        title.hash(&mut hasher);
        content.hash(&mut hasher);
    }
    hasher.finish()
}

static CACHE: Mutex<Option<CacheEntry>> = Mutex::new(None);

fn get_cached(cfg: &SystemConfig) -> Option<String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_millis() as i64;
    let current_key = cache_key(cfg);
    let cache = CACHE.lock().ok()?;
    if let Some(ref entry) = *cache {
        if entry.key == current_key && now < entry.expiry {
            return Some(entry.value.clone());
        }
    }
    None
}

fn set_cache(cfg: &SystemConfig, value: String) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let expiry = now + cfg.cache_ttl_ms as i64;
    let key = cache_key(cfg);
    if let Ok(mut cache) = CACHE.lock() {
        *cache = Some(CacheEntry { key, value, expiry });
    }
}

pub fn wrap_section(title: &str, content: &str) -> String {
    format!("<{}>\n{}\n</{}>", title, content, title)
}

pub fn cleanup_empty_lines(text: &str) -> String {
    let mut result = String::new();
    let mut prev_empty = false;
    for line in text.lines() {
        if line.trim().is_empty() {
            if !prev_empty {
                result.push('\n');
                prev_empty = true;
            }
        } else {
            result.push_str(line);
            result.push('\n');
            prev_empty = false;
        }
    }
    while result.ends_with('\n') {
        result.pop();
    }
    result
}

pub fn build_system_context(cfg: &SystemConfig) -> String {
    if let Some(cached) = get_cached(cfg) {
        return cached;
    }

    let mut sections: Vec<String> = Vec::new();

    if cfg.include_time {
        let now = chrono::Local::now();
        let formatted = now.format("%Y-%m-%d %H:%M:%S %z").to_string();
        sections.push(wrap_section("current_time", &formatted));
    }

    if cfg.include_env {
        let mut env_parts: Vec<String> = Vec::new();
        env_parts.push(format!("Platform: {}", std::env::consts::OS));
        env_parts.push(format!("Architecture: {}", std::env::consts::ARCH));
        if let Ok(cwd) = std::env::current_dir() {
            env_parts.push(format!("Working directory: {}", cwd.display()));
        }
        if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
            env_parts.push(format!("Home directory: {}", home));
        }
        sections.push(wrap_section("environment", &env_parts.join("\n")));
    }

    if cfg.include_tool_descriptions && !cfg.tool_descriptions.is_empty() {
        let tools_text = render_tool_descriptions(&cfg.tool_descriptions, ToolFormat::Compact);
        sections.push(wrap_section("available_tools", &tools_text));
    }

    if cfg.include_skills && !cfg.skills.is_empty() {
        let skills_text = cfg.skills.join("\n");
        sections.push(wrap_section("skills", &skills_text));
    }

    if cfg.include_workflows && !cfg.workflows.is_empty() {
        let workflows_text = cfg.workflows.join("\n");
        sections.push(wrap_section("workflows", &workflows_text));
    }

    for (title, content) in &cfg.custom_sections {
        sections.push(wrap_section(title, content));
    }

    let result = sections.join("\n\n");
    let result = cleanup_empty_lines(&result);

    set_cache(cfg, result.clone());
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_context_includes_time() {
        let cfg = SystemConfig {
            include_time: true,
            include_env: false,
            include_tool_descriptions: false,
            include_skills: false,
            include_workflows: false,
            timezone: None,
            cache_ttl_ms: 60_000,
            tool_descriptions: Vec::new(),
            skills: Vec::new(),
            workflows: Vec::new(),
            custom_sections: Vec::new(),
        };
        let ctx = build_system_context(&cfg);
        assert!(ctx.contains("current_time"));
    }

    #[test]
    fn test_system_context_includes_env() {
        let cfg = SystemConfig {
            include_time: false,
            include_env: true,
            include_tool_descriptions: false,
            include_skills: false,
            include_workflows: false,
            timezone: None,
            cache_ttl_ms: 60_000,
            tool_descriptions: Vec::new(),
            skills: Vec::new(),
            workflows: Vec::new(),
            custom_sections: Vec::new(),
        };
        let ctx = build_system_context(&cfg);
        assert!(ctx.contains("environment"));
    }

    #[test]
    fn test_system_context_uses_cache() {
        let cfg = SystemConfig::default();
        let first = build_system_context(&cfg);
        let second = build_system_context(&cfg);
        assert_eq!(first, second);
    }

    #[test]
    fn test_wrap_section() {
        let result = wrap_section("test", "content");
        assert_eq!(result, "<test>\ncontent\n</test>");
    }

    #[test]
    fn test_cleanup_empty_lines() {
        let input = "a\n\n\nb\n\nc";
        let result = cleanup_empty_lines(input);
        assert_eq!(result, "a\n\nb\n\nc");
    }
}
