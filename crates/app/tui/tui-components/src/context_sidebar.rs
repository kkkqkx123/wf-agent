//! Context sidebar panels for the session workspace.

use super::footer::format_tokens_compact;
use tui_core::reducer::UsageMeta;
use tui_core::screen_data::short_id;

/// Display snapshot feeding the Context section: the last-request usage
/// tally plus session identity. Breakdown rows stay hidden when the
/// provider did not report them; the window percentage hides when the
/// model window is unknown.
#[derive(Debug, Clone, Default)]
pub struct ContextSnapshot {
    pub usage: Option<UsageMeta>,
    pub context_window: Option<u32>,
    pub iteration: u32,
    pub message_count: u32,
    pub execution_id: Option<String>,
    pub model: Option<String>,
}

/// Width of the ASCII usage bar in the Context section.
pub const CONTEXT_BAR_WIDTH: usize = 10;

/// Usage bar for a window fraction in `[0, 1]`: filled vs empty blocks.
pub fn context_bar(fraction: f64, width: usize) -> String {
    let clamped = fraction.clamp(0.0, 1.0);
    let filled = (clamped * width as f64).round() as usize;
    let filled = filled.min(width);
    format!(
        "{}{}",
        "█".repeat(filled),
        "░".repeat(width.saturating_sub(filled))
    )
}

/// Plain lines for a [`ContextSnapshot`]: token split, cache and
/// reasoning rows only when reported, window percentage and bar only
/// when the window is known, identity row when any identity is present.
pub fn context_lines(snapshot: &ContextSnapshot) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(usage) = &snapshot.usage {
        let used = usage.used_tokens();
        lines.push(format!(
            "{} in · {} out ({} used)",
            format_tokens_compact(u64::from(usage.prompt_tokens)),
            format_tokens_compact(u64::from(usage.completion_tokens)),
            format_tokens_compact(used),
        ));
        if let Some(reasoning) = usage.reasoning_tokens {
            lines.push(format!(
                "reasoning {}",
                format_tokens_compact(u64::from(reasoning))
            ));
        }
        match (
            usage.cache_read_tokens,
            usage.cache_write_tokens,
            usage.cache_hit_rate(),
        ) {
            (Some(read), write, Some(rate)) => {
                let mut row = format!(
                    "cache {} hit ({:.0}%)",
                    format_tokens_compact(u64::from(read)),
                    rate * 100.0,
                );
                if let Some(write) = write {
                    row.push_str(&format!(
                        " · {} write",
                        format_tokens_compact(u64::from(write))
                    ));
                }
                lines.push(row);
            }
            (None, Some(write), _) => {
                lines.push(format!(
                    "cache {} write",
                    format_tokens_compact(u64::from(write))
                ));
            }
            _ => {}
        }
        match usage.window_fraction(snapshot.context_window) {
            Some(fraction) => {
                let window = snapshot.context_window.unwrap_or(0);
                lines.push(format!(
                    "{} {} of {} ({:.0}% used)",
                    context_bar(fraction, CONTEXT_BAR_WIDTH),
                    format_tokens_compact(used),
                    format_tokens_compact(u64::from(window)),
                    fraction * 100.0,
                ));
            }
            None => {
                lines.push("window unknown".to_string());
            }
        }
        if let Some(cost) = usage.cost {
            lines.push(format!("${cost:.4} spent"));
        }
    }
    let mut identity = Vec::new();
    if snapshot.iteration > 0 {
        identity.push(format!("iter {}", snapshot.iteration));
    }
    if snapshot.message_count > 0 {
        identity.push(format!("msgs {}", snapshot.message_count));
    }
    if let Some(id) = snapshot.execution_id.as_deref().filter(|id| !id.is_empty()) {
        identity.push(format!("exec {}", short_id(id)));
    }
    if let Some(model) = snapshot.model.as_deref().filter(|model| !model.is_empty()) {
        identity.push(format!("model {model}"));
    }
    if !identity.is_empty() {
        lines.push(identity.join(" · "));
    }
    lines
}

/// One context sidebar section.
#[derive(Debug, Clone, Default)]
pub struct ContextSection {
    pub title: String,
    pub lines: Vec<String>,
}

impl ContextSection {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            lines: Vec::new(),
        }
    }

    pub fn with_lines(title: impl Into<String>, lines: Vec<String>) -> Self {
        Self {
            title: title.into(),
            lines,
        }
    }

    pub fn push(&mut self, line: impl Into<String>) {
        self.lines.push(line.into());
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

/// Session-workspace context sidebar combining several panels.
#[derive(Debug, Clone, Default)]
pub struct ContextSidebar {
    pub files: ContextSection,
    pub todos: ContextSection,
    pub context: ContextSection,
    pub mcp: ContextSection,
    pub git: ContextSection,
}

impl ContextSidebar {
    pub fn new() -> Self {
        let mut sidebar = Self::default();
        sidebar.files.title = "Files".to_string();
        sidebar.todos.title = "Todos".to_string();
        sidebar.context.title = "Context".to_string();
        sidebar.mcp.title = "MCP".to_string();
        sidebar.git.title = "Git".to_string();
        sidebar
    }

    /// All non-empty sections in stable order.
    pub fn visible_sections(&self) -> Vec<&ContextSection> {
        [
            &self.files,
            &self.todos,
            &self.context,
            &self.mcp,
            &self.git,
        ]
        .into_iter()
        .filter(|section| !section.is_empty())
        .collect()
    }

    /// Render the sidebar to plain lines for tests and headless use.
    pub fn render_lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        for section in self.visible_sections() {
            out.push(format!("## {}", section.title));
            for line in &section.lines {
                out.push(format!("- {line}"));
            }
        }
        out
    }

    /// Rebuild the Context section from a display snapshot. An empty
    /// snapshot clears the section so it hides with the other empty ones.
    pub fn set_context_snapshot(&mut self, snapshot: &ContextSnapshot) {
        self.context.lines = context_lines(snapshot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tui_core::reducer::UsageMeta;

    fn usage() -> UsageMeta {
        UsageMeta {
            prompt_tokens: 12_000,
            completion_tokens: 4_000,
            cost: Some(0.0025),
            reasoning_tokens: Some(1_000),
            cache_read_tokens: Some(8_000),
            cache_write_tokens: Some(500),
            total_tokens: Some(17_500),
        }
    }

    #[test]
    fn empty_sections_are_hidden() {
        let mut sidebar = ContextSidebar::new();
        sidebar.files.push("main.rs".to_string());
        sidebar.git.push("clean".to_string());
        let lines = sidebar.render_lines();
        assert_eq!(lines, vec!["## Files", "- main.rs", "## Git", "- clean"]);
    }

    #[test]
    fn sections_preserve_order() {
        assert_eq!(ContextSidebar::new().visible_sections().len(), 0);
    }

    #[test]
    fn context_bar_scales_with_fraction() {
        assert_eq!(context_bar(0.0, 10), "░░░░░░░░░░");
        assert_eq!(context_bar(1.0, 10), "██████████");
        assert_eq!(context_bar(0.5, 10), "█████░░░░░");
        assert_eq!(context_bar(2.0, 4), "████");
        assert_eq!(context_bar(-1.0, 4), "░░░░");
    }

    #[test]
    fn context_snapshot_renders_full_breakdown() {
        let snapshot = ContextSnapshot {
            usage: Some(usage()),
            context_window: Some(100_000),
            iteration: 3,
            message_count: 12,
            execution_id: Some("exec-a1b2c3d4e5".to_string()),
            model: Some("claude-4-5".to_string()),
        };
        let lines = context_lines(&snapshot);
        assert_eq!(lines[0], "12K in · 4K out (17.5K used)");
        assert_eq!(lines[1], "reasoning 1K");
        assert!(lines[2].starts_with("cache 8K hit (67%)"));
        assert!(lines[2].contains("500 write"));
        assert!(lines[3].starts_with("██"));
        assert!(lines[3].contains("17.5K of 100K (18% used)"));
        assert_eq!(lines[4], "$0.0025 spent");
        assert_eq!(lines[5], "iter 3 · msgs 12 · exec exec-a1b... · model claude-4-5");
    }

    #[test]
    fn context_snapshot_degrades_without_window_or_cache() {
        let snapshot = ContextSnapshot {
            usage: Some(UsageMeta {
                prompt_tokens: 500,
                completion_tokens: 60,
                cost: None,
                reasoning_tokens: None,
                cache_read_tokens: None,
                cache_write_tokens: None,
                total_tokens: None,
            }),
            context_window: None,
            iteration: 0,
            message_count: 0,
            execution_id: None,
            model: None,
        };
        let lines = context_lines(&snapshot);
        assert_eq!(lines, vec!["500 in · 60 out (560 used)", "window unknown"]);
    }

    #[test]
    fn context_snapshot_roundtrips_through_sidebar() {
        let mut sidebar = ContextSidebar::new();
        assert!(sidebar.context.is_empty());
        sidebar.set_context_snapshot(&ContextSnapshot {
            usage: Some(usage()),
            context_window: Some(200_000),
            iteration: 1,
            message_count: 2,
            execution_id: None,
            model: None,
        });
        let rendered = sidebar.render_lines();
        assert!(rendered.iter().any(|line| line == "## Context"));
        assert!(rendered.iter().any(|line| line.contains("12K in")));
        sidebar.set_context_snapshot(&ContextSnapshot::default());
        assert!(sidebar.context.is_empty());
    }
}
