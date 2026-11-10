//! Collapsed tool-output rendering shared by the live transcript and the
//! replay loader.
//!
//! These are pure width-independent text transforms: truncation only shapes
//! the display rows, while the stored block excerpt keeps the capped source
//! text. Counts are grapheme-aware so CJK and wide glyphs are never split.
//!
//! Tool-name matching here is a frontend-local fallback for persisted
//! history and foreign servers. The live path prefers the protocol display
//! payload when the server provides one.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;
use wf_types::tool::DisplayKind;

/// Detail level driving tool-block grouping. Thresholds stay orthogonal:
/// the level decides grouping and default expansion, never preview length.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DetailLevel {
    /// Consecutive tool blocks merge into one activity summary.
    Low,
    /// Default: one summary plus a collapsed preview per tool.
    #[default]
    Medium,
    /// Exploration blocks render expanded by default.
    High,
}

impl DetailLevel {
    /// Cycle Low -> Medium -> High -> Low for the level key.
    pub fn cycle(self) -> Self {
        match self {
            Self::Low => Self::Medium,
            Self::Medium => Self::High,
            Self::High => Self::Low,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

/// Runtime thresholds for tool-output rendering. The single convergence
/// point for preview lengths; call sites only pass the tool kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolOutputConfig {
    /// Master switch. Off restores the legacy one-line lifecycle rows.
    pub enabled: bool,
    /// Generic preview: head lines kept.
    pub preview_max_lines: usize,
    /// Generic preview: grapheme budget applied after the line cut.
    pub preview_max_chars: usize,
    /// Shell preview: tail lines kept.
    pub shell_preview_lines: usize,
    /// Failure preview: head lines of the error text kept.
    pub error_preview_lines: usize,
    /// Per-block stored excerpt cap in bytes (tail-kept).
    pub block_max_bytes: usize,
    /// Default expansion for exploration blocks.
    pub level: DetailLevel,
}

impl Default for ToolOutputConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            preview_max_lines: 10,
            preview_max_chars: 320,
            shell_preview_lines: 10,
            error_preview_lines: 4,
            block_max_bytes: 32 * 1024,
            level: DetailLevel::Medium,
        }
    }
}

/// Collapsed preview plus overflow accounting for the summary hint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Collapsed {
    pub text: String,
    pub overflow: bool,
    pub hidden_lines: usize,
}

/// Shell-family tool names, mirroring the engine fold routing shape.
pub fn is_shell_tool_name(name: &str) -> bool {
    matches!(
        name,
        "execute_command"
            | "backend_shell"
            | "shell_output"
            | "shell_wait"
            | "shell_kill"
            | "shell_send_input"
            | "shell_resize"
            | "get_or_create_shell"
            | "execute_in_session"
            | "release_sessions_for_task"
            | "session_observe"
            | "shell_tasks"
            | "shell_background"
    )
}

/// Read and retrieval tool names whose bodies stay hidden by default.
pub fn is_exploration_tool_name(name: &str) -> bool {
    matches!(
        name,
        "read_file"
            | "read_file_folded"
            | "list_files"
            | "glob_search"
            | "grep_search"
            | "code_search"
            | "code_keyword_search"
            | "retrieve_code"
            | "web_fetch"
            | "web_search"
    )
}

fn is_edit_tool_name(name: &str) -> bool {
    matches!(
        name,
        "write_file" | "edit_file" | "apply_diff" | "apply_patch"
    )
}

/// Infer the presentation kind from a tool name. Used when no protocol
/// display payload is available.
pub fn infer_kind(tool_name: &str) -> DisplayKind {
    if is_shell_tool_name(tool_name) {
        DisplayKind::ShellTail
    } else if is_edit_tool_name(tool_name) {
        DisplayKind::Diff
    } else if is_exploration_tool_name(tool_name) {
        DisplayKind::Count
    } else {
        DisplayKind::Text
    }
}

/// Cut `text` to `max_width` columns on grapheme boundaries.
pub fn truncate_graphemes(text: &str, max_width: usize) -> String {
    let mut out = String::new();
    let mut width = 0usize;
    for grapheme in text.graphemes(true) {
        let grapheme_width = grapheme.width();
        if width + grapheme_width > max_width {
            break;
        }
        out.push_str(grapheme);
        width += grapheme_width;
    }
    out
}

fn clamp_chars(text: &str, max_chars: usize) -> (String, bool) {
    if text.graphemes(true).count() <= max_chars {
        return (text.to_string(), false);
    }
    let kept: String = text.graphemes(true).take(max_chars).collect();
    (format!("{kept}…"), true)
}

/// Keep the head of `text` within the line and grapheme budgets. Overlong
/// input marks the last visible line and reports the hidden line count.
pub fn collapse_output(text: &str, max_lines: usize, max_chars: usize) -> Collapsed {
    let lines: Vec<&str> = text.lines().collect();
    let capped_lines = max_lines.max(1);
    if lines.len() <= capped_lines && text.graphemes(true).count() <= max_chars {
        return Collapsed {
            text: text.to_string(),
            overflow: false,
            hidden_lines: 0,
        };
    }
    let hidden_lines = lines.len().saturating_sub(capped_lines);
    let mut visible: Vec<String> = lines
        .into_iter()
        .take(capped_lines)
        .map(str::to_string)
        .collect();
    if hidden_lines > 0 {
        if let Some(last) = visible.last_mut() {
            last.push('…');
        }
    }
    let (clamped, _) = clamp_chars(&visible.join("\n"), max_chars);
    Collapsed {
        text: clamped,
        overflow: true,
        hidden_lines,
    }
}

/// Keep the tail of `text` within the budgets, heading it with the hidden
/// line count. Command output converges on recent lines, so the head is
/// the side that is dropped.
pub fn collapse_shell_tail(text: &str, max_lines: usize, max_chars: usize) -> Collapsed {
    let lines: Vec<&str> = text.lines().collect();
    let capped_lines = max_lines.max(1);
    if lines.len() <= capped_lines && text.graphemes(true).count() <= max_chars {
        return Collapsed {
            text: text.to_string(),
            overflow: false,
            hidden_lines: 0,
        };
    }
    let hidden_lines = lines.len().saturating_sub(capped_lines);
    let tail: Vec<&str> = lines
        .into_iter()
        .rev()
        .take(capped_lines)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let composed = if hidden_lines > 0 {
        format!("({hidden_lines} earlier lines)\n{}", tail.join("\n"))
    } else {
        tail.join("\n")
    };
    let (clamped, _) = clamp_chars(&composed, max_chars);
    Collapsed {
        text: clamped,
        overflow: true,
        hidden_lines,
    }
}

/// Cap one stored block excerpt to `max_bytes`, keeping the tail on a char
/// boundary. Returns the excerpt and whether the cap was hit.
pub fn cap_block_bytes(text: &str, max_bytes: usize) -> (String, bool) {
    if text.len() <= max_bytes {
        return (text.to_string(), false);
    }
    let mut start = text.len().saturating_sub(max_bytes.max(1));
    while start < text.len() && !text.is_char_boundary(start) {
        start += 1;
    }
    (
        format!("[earlier output omitted]\n{}", &text[start..]),
        true,
    )
}

/// Summarize consecutive tool activity for the low detail level, e.g.
/// `3 commands · 1 edit · 2 reads`.
pub fn summarize_activity(items: &[(String, bool)]) -> String {
    let mut commands = 0usize;
    let mut edits = 0usize;
    let mut reads = 0usize;
    let mut other = 0usize;
    for (name, _) in items {
        match infer_kind(name) {
            DisplayKind::ShellTail => commands += 1,
            DisplayKind::Diff => edits += 1,
            DisplayKind::Count => reads += 1,
            DisplayKind::Text => other += 1,
        }
    }
    let mut parts = Vec::new();
    if commands > 0 {
        parts.push(format!("{} command{}", commands, plural(commands)));
    }
    if edits > 0 {
        parts.push(format!("{} edit{}", edits, plural(edits)));
    }
    if reads > 0 {
        parts.push(format!("{} read{}", reads, plural(reads)));
    }
    if other > 0 {
        parts.push(format!("{} tool{}", other, plural(other)));
    }
    if parts.is_empty() {
        return "tools".to_string();
    }
    parts.join(" · ")
}

fn plural(count: usize) -> &'static str {
    if count == 1 {
        ""
    } else {
        "s"
    }
}

/// Style decision for one preview line. Diff hunks keep their add/remove
/// accent so unified diffs read as diffs; everything else stays muted.
/// The caller maps the returned marker to its own role type so this crate
/// never depends on the component layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewTone {
    Muted,
    Add,
    Remove,
    Accent,
}

/// Classify one preview line under its display kind.
pub fn preview_tone(kind: DisplayKind, line: &str) -> PreviewTone {
    if kind != DisplayKind::Diff {
        return PreviewTone::Muted;
    }
    if line.starts_with("@@") {
        return PreviewTone::Accent;
    }
    if line.starts_with('+') {
        return PreviewTone::Add;
    }
    if line.starts_with('-') {
        return PreviewTone::Remove;
    }
    PreviewTone::Muted
}

/// Declarative tool summary contributed by a plugin: the host
/// substitutes `{args}` (compact argument text) and `{result}` (truncated
/// result text) and renders the outcome with the theme. Unknown
/// placeholders stay verbatim so typos stay visible.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolSummaryTemplate {
    /// Tool name this summary renders.
    pub tool_name: String,
    /// Template for the pending/running call line.
    pub call_template: String,
    /// Template for the settled result line.
    pub result_template: String,
}

impl ToolSummaryTemplate {
    /// Convert one snapshot tool summary into a render template.
    /// Empty pairs never reach the registry, so this keeps every entry.
    pub fn from_snapshot(summary: &wf_types::cli::CliToolSummary) -> Self {
        Self {
            tool_name: summary.tool_name.clone(),
            call_template: summary.call_template.clone(),
            result_template: summary.result_template.clone(),
        }
    }

    /// Render the call portion: `{args}` takes the compact argument text.
    /// `None` means no template is set and the builtin card applies.
    pub fn render_call(&self, args: &str) -> Option<String> {
        (!self.call_template.is_empty()).then(|| {
            self.call_template
                .replace("{args}", args)
                .replace("{result}", "")
        })
    }
}

/// Apply snapshot Markdown rules for one transcript surface. Rules run in
/// order and only when their surface list is empty or names `surface`;
/// empty patterns are skipped so one bad entry never erases the document.
/// Display-only: the stored payload never changes.
pub fn apply_snapshot_markdown_rules(
    text: &str,
    rules: &[wf_types::cli::CliMarkdownRule],
    surface: &str,
) -> String {
    let mut out = text.to_string();
    for rule in rules {
        if rule.pattern.is_empty() {
            continue;
        }
        if !rule.surfaces.is_empty() && !rule.surfaces.iter().any(|s| s == surface) {
            continue;
        }
        out = out.replace(rule.pattern.as_str(), rule.replacement.as_str());
    }
    out
}

/// Assemble one tool summary row: status mark, tool name, optional call
/// summary, optional elapsed time and optional overflow suffix. Callers
/// pass the payload fields through unchanged; empty call summaries and
/// zero hidden lines contribute nothing.
pub fn format_tool_summary(
    tool_name: &str,
    call_summary: &str,
    elapsed_ms: Option<u64>,
    hidden_lines: usize,
    truncated: bool,
    success: bool,
) -> String {
    format_tool_summary_with(
        tool_name,
        call_summary,
        elapsed_ms,
        hidden_lines,
        truncated,
        success,
        &[],
    )
}

/// [`format_tool_summary`] with plugin templates: a matching template
/// replaces the parenthetical call portion; without a match (or with an
/// empty template) the builtin card applies, so a bad contribution
/// degrades to the default row instead of breaking the transcript.
pub fn format_tool_summary_with(
    tool_name: &str,
    call_summary: &str,
    elapsed_ms: Option<u64>,
    hidden_lines: usize,
    truncated: bool,
    success: bool,
    templates: &[ToolSummaryTemplate],
) -> String {
    let mark = if success {
        tui_style::icons::SUCCESS
    } else {
        tui_style::icons::FAILURE
    };
    let mut summary = format!("{mark} {tool_name}");
    let call = templates
        .iter()
        .find(|template| template.tool_name == tool_name)
        .and_then(|template| template.render_call(call_summary.trim()))
        .unwrap_or_else(|| {
            let call = call_summary.trim();
            if call.is_empty() {
                String::new()
            } else {
                format!("({call})")
            }
        });
    if !call.is_empty() {
        summary.push_str(&call);
    }
    if let Some(elapsed) = elapsed_ms {
        summary.push_str(&format!(" ({elapsed}ms)"));
    }
    if hidden_lines > 0 {
        summary.push_str(&format!(" · +{hidden_lines} lines"));
    } else if truncated {
        summary.push_str(" · +more");
    }
    summary
}

/// Width-aware footer segment packing: compact forms replace full forms
/// before any segment is shrunk or dropped. Lower priority segments drop
/// first; equal priorities keep input order. Returns one text per input
/// segment with empty strings marking dropped items.
pub fn fit_footer_segments(
    segments: &[(String, Option<String>, i32)],
    max_width: usize,
) -> Vec<String> {
    use unicode_width::UnicodeWidthStr;
    let mut texts: Vec<String> = segments.iter().map(|item| item.0.clone()).collect();
    let mut widths: Vec<usize> = texts.iter().map(|text| text.width()).collect();
    let total = |texts: &[String], widths: &[usize]| {
        widths.iter().sum::<usize>() + texts.len().saturating_sub(1)
    };
    if total(&texts, &widths) > max_width {
        let mut order: Vec<usize> = (0..segments.len()).collect();
        order.sort_by_key(|index| (segments[*index].2, *index));
        for index in order {
            if let Some(compact) = segments[index].1.clone() {
                if compact.width() < widths[index] {
                    texts[index] = compact.clone();
                    widths[index] = compact.width();
                }
            }
            if total(&texts, &widths) <= max_width {
                break;
            }
        }
    }
    while total(&texts, &widths) > max_width {
        let mut target: Option<usize> = None;
        for (index, text) in texts.iter().enumerate() {
            if text.is_empty() {
                continue;
            }
            let is_lower = target.is_none_or(|current| segments[index].2 < segments[current].2);
            if is_lower {
                target = Some(index);
            }
        }
        let Some(drop) = target else {
            break;
        };
        texts[drop].clear();
        widths[drop] = 0;
    }
    texts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_template_replaces_the_call_portion() {
        let templates = [ToolSummaryTemplate {
            tool_name: "grep".to_string(),
            call_template: "search {args}".to_string(),
            result_template: String::new(),
        }];
        let row = format_tool_summary_with("grep", "-r foo", None, 0, false, true, &templates);
        assert!(row.contains("grep"));
        assert!(row.contains("search -r foo"));
        assert!(!row.contains("-r foo)"));
    }

    #[test]
    fn missing_template_falls_back_to_the_builtin_card() {
        let plain = format_tool_summary("grep", "-r foo", None, 0, false, true);
        let fallback = format_tool_summary_with("grep", "-r foo", None, 0, false, true, &[]);
        assert_eq!(plain, fallback);
        assert!(fallback.contains("(-r foo)"));
    }

    #[test]
    fn short_text_passes_through_verbatim() {
        let collapsed = collapse_output("one\ntwo", 10, 320);
        assert_eq!(collapsed.text, "one\ntwo");
        assert!(!collapsed.overflow);
        assert_eq!(collapsed.hidden_lines, 0);
    }

    #[test]
    fn empty_text_has_no_overflow() {
        let collapsed = collapse_output("", 10, 320);
        assert_eq!(collapsed.text, "");
        assert!(!collapsed.overflow);
    }

    #[test]
    fn head_collapse_marks_last_line_and_counts_hidden() {
        let text = (0..25)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let collapsed = collapse_output(&text, 10, 10_000);
        assert!(collapsed.overflow);
        assert_eq!(collapsed.hidden_lines, 15);
        assert!(collapsed
            .text
            .lines()
            .next()
            .expect("first line")
            .contains("line 0"));
        assert!(collapsed.text.contains('…'));
    }

    #[test]
    fn char_budget_cuts_wide_text_without_splitting_glyphs() {
        let collapsed = collapse_output("你好世界hello", 10, 4);
        assert!(collapsed.overflow);
        assert_eq!(collapsed.hidden_lines, 0);
        assert!(collapsed.text.contains('…'));
        assert!(collapsed.text.graphemes(true).count() <= 5);
    }

    #[test]
    fn shell_tail_keeps_recent_lines_with_head_marker() {
        let text = (0..30)
            .map(|i| format!("out {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let collapsed = collapse_shell_tail(&text, 10, 10_000);
        assert!(collapsed.overflow);
        assert_eq!(collapsed.hidden_lines, 20);
        assert!(collapsed.text.contains("(20 earlier lines)"));
        assert!(collapsed.text.contains("out 29"));
        assert!(!collapsed.text.contains("out 0\n"));
    }

    #[test]
    fn shell_tail_short_output_is_verbatim() {
        let collapsed = collapse_shell_tail("a\nb", 10, 320);
        assert_eq!(collapsed.text, "a\nb");
        assert!(!collapsed.overflow);
    }

    #[test]
    fn block_cap_keeps_tail_on_char_boundary() {
        let (capped, hit) = cap_block_bytes("hello world", 5);
        assert!(hit);
        assert!(capped.contains("earlier output omitted"));
        assert!(capped.ends_with("world"));
        let (same, miss) = cap_block_bytes("hi", 5);
        assert!(!miss);
        assert_eq!(same, "hi");
    }

    #[test]
    fn block_cap_never_splits_multibyte_chars() {
        let text = "你好世界";
        let (capped, hit) = cap_block_bytes(text, 7);
        assert!(hit);
        let body = capped.split_once('\n').expect("marker line").1;
        assert!(body.is_char_boundary(0));
    }

    #[test]
    fn kind_inference_covers_shell_edit_and_exploration() {
        assert_eq!(infer_kind("execute_command"), DisplayKind::ShellTail);
        assert_eq!(infer_kind("edit_file"), DisplayKind::Diff);
        assert_eq!(infer_kind("read_file"), DisplayKind::Count);
        assert_eq!(infer_kind("grep_search"), DisplayKind::Count);
        assert_eq!(infer_kind("mystery_tool"), DisplayKind::Text);
    }

    #[test]
    fn activity_summary_counts_by_kind() {
        let items = vec![
            ("execute_command".to_string(), true),
            ("execute_command".to_string(), false),
            ("edit_file".to_string(), true),
            ("read_file".to_string(), true),
        ];
        assert_eq!(summarize_activity(&items), "2 commands · 1 edit · 1 read");
        let single = vec![("read_file".to_string(), true)];
        assert_eq!(summarize_activity(&single), "1 read");
        assert_eq!(summarize_activity(&[]), "tools");
    }

    #[test]
    fn detail_level_cycles_and_labels() {
        assert_eq!(DetailLevel::Low.cycle(), DetailLevel::Medium);
        assert_eq!(DetailLevel::Medium.cycle(), DetailLevel::High);
        assert_eq!(DetailLevel::High.cycle(), DetailLevel::Low);
        assert_eq!(DetailLevel::default(), DetailLevel::Medium);
        assert_eq!(DetailLevel::High.label(), "high");
    }

    #[test]
    fn snapshot_markdown_rules_filter_by_surface_and_skip_empty() {
        let rules = vec![
            wf_types::cli::CliMarkdownRule {
                id: "arrows".to_string(),
                pattern: "-->".to_string(),
                replacement: "\u{2192}".to_string(),
                surfaces: vec!["assistant".to_string()],
            },
            wf_types::cli::CliMarkdownRule {
                id: "empty".to_string(),
                pattern: String::new(),
                replacement: "gone".to_string(),
                surfaces: vec![],
            },
        ];
        assert_eq!(
            apply_snapshot_markdown_rules("a --> b", &rules, "assistant"),
            "a \u{2192} b"
        );
        assert_eq!(
            apply_snapshot_markdown_rules("a --> b", &rules, "user"),
            "a --> b"
        );
    }
}
