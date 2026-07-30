use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use wf_types::tool_description::ToolDescriptionData;
use wf_types::TodoItem;

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

#[derive(Debug, Clone, Default)]
pub struct UserInput {
    pub todos: Vec<TodoItem>,
    pub pinned: Vec<PathBuf>,
    pub tree: Option<String>,
    pub custom_data: Option<HashMap<String, String>>,
}

struct CacheEntry {
    value: String,
    expiry: i64,
}

static CACHE_KEY: AtomicI64 = AtomicI64::new(0);

fn cache_key(cfg: &SystemConfig) -> i64 {
    let mut key = 0i64;
    key = key.wrapping_mul(31).wrapping_add(cfg.include_time as i64);
    key = key.wrapping_mul(31).wrapping_add(cfg.include_env as i64);
    key = key.wrapping_mul(31).wrapping_add(cfg.include_tool_descriptions as i64);
    key = key.wrapping_mul(31).wrapping_add(cfg.include_skills as i64);
    key = key.wrapping_mul(31).wrapping_add(cfg.include_workflows as i64);
    key
}

static mut CACHE: Option<CacheEntry> = None;

fn get_cached(cfg: &SystemConfig) -> Option<String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_millis() as i64;
    let current_key = cache_key(cfg);
    if CACHE_KEY.load(Ordering::Relaxed) == current_key {
        unsafe {
            if let Some(ref entry) = CACHE {
                if now < entry.expiry {
                    return Some(entry.value.clone());
                }
            }
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
    CACHE_KEY.store(key, Ordering::Relaxed);
    unsafe {
        CACHE = Some(CacheEntry { value, expiry });
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

pub fn build_user_context(input: &UserInput) -> String {
    let mut sections: Vec<String> = Vec::new();

    if !input.todos.is_empty() {
        let mut todo_lines: Vec<String> = Vec::new();
        todo_lines.push("TODO list:".into());
        for item in &input.todos {
            let status = match item.status {
                wf_types::TodoStatus::Pending => "[ ]",
                wf_types::TodoStatus::InProgress => "[~]",
                wf_types::TodoStatus::Completed => "[x]",
                wf_types::TodoStatus::Cancelled => "[-]",
            };
            todo_lines.push(format!("  {} {}", status, item.content));
        }
        sections.push(todo_lines.join("\n"));
    }

    if !input.pinned.is_empty() {
        let mut pinned_lines: Vec<String> = Vec::new();
        pinned_lines.push("Pinned files:".into());
        for path in &input.pinned {
            pinned_lines.push(format!("  - {}", path.display()));
        }
        sections.push(pinned_lines.join("\n"));
    }

    if let Some(ref tree) = input.tree {
        sections.push(format!("Workspace structure:\n{}", tree));
    }

    if let Some(ref custom) = input.custom_data {
        for (key, value) in custom {
            sections.push(format!("{}: {}", key, value));
        }
    }

    sections.join("\n\n")
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
    fn test_user_context_empty() {
        let input = UserInput::default();
        let ctx = build_user_context(&input);
        assert_eq!(ctx, "");
    }

    #[test]
    fn test_user_context_with_todos() {
        let input = UserInput {
            todos: vec![
                TodoItem {
                    id: "1".into(),
                    content: "Task one".into(),
                    status: wf_types::TodoStatus::Pending,
                    priority: None,
                    created_at: None,
                    updated_at: None,
                    metadata: None,
                },
                TodoItem {
                    id: "2".into(),
                    content: "Task two".into(),
                    status: wf_types::TodoStatus::Completed,
                    priority: None,
                    created_at: None,
                    updated_at: None,
                    metadata: None,
                },
            ],
            pinned: Vec::new(),
            tree: None,
            custom_data: None,
        };
        let ctx = build_user_context(&input);
        assert!(ctx.contains("TODO list:"));
        assert!(ctx.contains("[ ] Task one"));
        assert!(ctx.contains("[x] Task two"));
    }

    #[test]
    fn test_user_context_with_pinned() {
        let input = UserInput {
            todos: Vec::new(),
            pinned: vec![PathBuf::from("/home/user/project/src/main.rs")],
            tree: None,
            custom_data: None,
        };
        let ctx = build_user_context(&input);
        assert!(ctx.contains("Pinned files:"));
        assert!(ctx.contains("main.rs"));
    }

    #[test]
    fn test_user_context_with_tree() {
        let input = UserInput {
            todos: Vec::new(),
            pinned: Vec::new(),
            tree: Some("src/\n  main.rs\n  lib.rs".into()),
            custom_data: None,
        };
        let ctx = build_user_context(&input);
        assert!(ctx.contains("Workspace structure:"));
        assert!(ctx.contains("main.rs"));
    }

    #[test]
    fn test_user_context_with_custom_data() {
        let input = UserInput {
            todos: Vec::new(),
            pinned: Vec::new(),
            tree: None,
            custom_data: Some(HashMap::from([("project".into(), "wf-agent".into())])),
        };
        let ctx = build_user_context(&input);
        assert!(ctx.contains("project: wf-agent"));
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
