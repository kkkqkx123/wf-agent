//! Mention parsing for the TUI footer composer (inline form): `@` file /
//! directory / skill / workflow / tool / script / resource / agent mentions.
//!
//! The composer highlights mentions as intervals `Vec<(Range<usize>, MentionKind)>`
//! where the range is byte offsets into the buffer. The streaming view never
//! mutates them: edits recompute the intervals from the current buffer text.
//!
//! Syntax:
//! * `@path/to/file` — file mention, optionally with line range
//!   `file:#10`, `file:#10-20`, `file:10`, `file:10-20`, `file#10`, `file#10-20`;
//!   a trailing `/` classifies the reference as a directory
//! * `@skill:name` or `skill:name` after `@` — skill mention
//! * `@workflow:id` / `@wf:id` / `workflow:id` — workflow mention
//! * bare `@name` without prefix is treated as a file when it contains a `/`
//!   or `.` , otherwise as a skill when it matches a known skill, otherwise
//!   as a workflow, finally as a file.
//!
//! File list is produced by scanning `project_root` (or the current directory
//! when unavailable) through the shared workspace scan contract.

use std::ops::Range;
use std::path::Path;

use ratatui::style::Style;
use ratatui::text::Span;
use tui_style::theme::{ColorRole, Theme};

/// Kind of a mention interval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MentionKind {
    File {
        path: String,
        lines: Option<(u32, u32)>,
    },
    Directory {
        path: String,
    },
    Skill {
        name: String,
    },
    Workflow {
        id: String,
    },
    Other {
        kind: String,
        value: String,
    },
}

/// Semantic tone of a mention, sharing its names with the web mention
/// palette so both surfaces classify the same reference identically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MentionTone {
    File,
    Directory,
    Skill,
    Workflow,
    Agent,
    Tool,
    Script,
    Resource,
    Neutral,
}

impl MentionTone {
    /// Group order used by the mention panel.
    pub const ORDER: [MentionTone; 9] = [
        MentionTone::File,
        MentionTone::Directory,
        MentionTone::Skill,
        MentionTone::Workflow,
        MentionTone::Resource,
        MentionTone::Tool,
        MentionTone::Script,
        MentionTone::Agent,
        MentionTone::Neutral,
    ];

    /// Short group title shown above the tone's rows.
    pub fn title(self) -> &'static str {
        match self {
            MentionTone::File => "files",
            MentionTone::Directory => "directories",
            MentionTone::Skill => "skills",
            MentionTone::Workflow => "workflows",
            MentionTone::Agent => "agents",
            MentionTone::Tool => "tools",
            MentionTone::Script => "scripts",
            MentionTone::Resource => "resources",
            MentionTone::Neutral => "mentions",
        }
    }

    /// The single `MentionTone -> ColorRole` mapping table.
    pub fn role(self) -> ColorRole {
        match self {
            MentionTone::File | MentionTone::Resource => ColorRole::Accent,
            MentionTone::Directory | MentionTone::Agent => ColorRole::Add,
            MentionTone::Skill | MentionTone::Workflow => ColorRole::Highlight,
            MentionTone::Tool | MentionTone::Script => ColorRole::Warning,
            MentionTone::Neutral => ColorRole::Muted,
        }
    }

    /// Single-width glyph shown before a reference in the mention panel.
    /// Each arm documents the corresponding Web icon name from
    /// `@wf-agent/ui/icons` so the two surfaces stay comparable.
    pub fn icon(self) -> char {
        match self {
            MentionTone::File => '▢',      // web: file
            MentionTone::Directory => '▤', // web: folder
            MentionTone::Skill => '✦',     // web: sparkles
            MentionTone::Workflow => '⟳',  // web: workflow
            MentionTone::Agent => '☻',     // web: user
            MentionTone::Tool => '⚙',      // web: terminal
            MentionTone::Script => '❯',    // web: code
            MentionTone::Resource => '⇄',  // web: link
            MentionTone::Neutral => '#',   // web: hash
        }
    }

    /// Foreground style for this tone, resolved through the dark palette so
    /// the render pipeline can adapt it to light themes.
    pub fn style(self) -> Style {
        Theme::dark_default().style_for_role(self.role())
    }
}

impl MentionKind {
    /// Tone of this mention, mirroring the web `mentionTone` mapping.
    pub fn tone(&self) -> MentionTone {
        match self {
            MentionKind::File { path, .. } => file_tone(path),
            MentionKind::Directory { .. } => MentionTone::Directory,
            MentionKind::Skill { .. } => MentionTone::Skill,
            MentionKind::Workflow { .. } => MentionTone::Workflow,
            MentionKind::Other { kind, value } => mention_tone(kind, value),
        }
    }
}

/// A file path ending in `/` is a directory reference.
fn file_tone(path: &str) -> MentionTone {
    if path.trim_end().ends_with('/') {
        MentionTone::Directory
    } else {
        MentionTone::File
    }
}

/// Map a raw mention kind and its value to a tone, mirroring the web mapping.
pub fn mention_tone(kind: &str, value: &str) -> MentionTone {
    match kind {
        "file" => file_tone(value),
        "wf" | "workflow" => MentionTone::Workflow,
        "skill" => MentionTone::Skill,
        "agent" | "agent_loop" => MentionTone::Agent,
        "tool" => MentionTone::Tool,
        "script" => MentionTone::Script,
        "resource" => MentionTone::Resource,
        _ => MentionTone::Neutral,
    }
}

/// One parsed mention with its byte range in the source text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mention {
    pub range: Range<usize>,
    pub kind: MentionKind,
    /// Raw matched text (including the leading `@`).
    pub raw: String,
}

/// The single `MentionKind -> ColorRole` mapping table.
pub fn mention_role(kind: &MentionKind) -> ColorRole {
    kind.tone().role()
}

/// Semantic foreground style for TUI rendering, resolved through the dark
/// palette so the render pipeline can adapt it to light themes.
pub fn mention_style(kind: &MentionKind) -> Style {
    kind.tone().style()
}

/// Single-width glyph shown before a mention reference in the panel.
pub fn mention_icon(kind: &MentionKind) -> char {
    kind.tone().icon()
}

/// Staged attachment placeholder kind. Labels stay plain text in the
/// composer buffer so edits recompute their intervals, mirroring the
/// `@mention` approach instead of tracking extmarks through edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentKind {
    Image,
    File,
}

impl AttachmentKind {
    /// Visible label prefix used inside the placeholder brackets.
    pub fn tag(self) -> &'static str {
        match self {
            AttachmentKind::Image => "Image",
            AttachmentKind::File => "File",
        }
    }
}

/// One staged attachment placeholder with its byte range in the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentSpan {
    pub range: Range<usize>,
    pub kind: AttachmentKind,
    pub index: usize,
    pub label: String,
}

/// Visible placeholder label for the nth staged entry (1-based).
/// The trailing space is added by the inserter, not stored here.
pub fn attachment_label(kind: AttachmentKind, index: usize) -> String {
    format!("[{} {}]", kind.tag(), index.max(1))
}

/// Parse all staged attachment placeholders in `text`.
/// Only `[Image N]` and `[File N]` with a positive number match so
/// ordinary bracketed prose never becomes a placeholder.
pub fn parse_attachments(text: &str) -> Vec<AttachmentSpan> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            let rest = &text[i..];
            if let Some(span) = match_attachment_at(rest, i) {
                let end = span.range.end;
                out.push(span);
                i = end;
                continue;
            }
        }
        i += 1;
    }
    out
}

fn match_attachment_at(rest: &str, base: usize) -> Option<AttachmentSpan> {
    let (tag, index, len) = wf_common::mention::staged_placeholder_parts(rest)?;
    let kind = match tag {
        "Image" => AttachmentKind::Image,
        _ => AttachmentKind::File,
    };
    let label = attachment_label(kind, index);
    if rest.len() < len || !rest.starts_with(label.as_str()) {
        return None;
    }
    Some(AttachmentSpan {
        range: base..base + len,
        kind,
        index,
        label,
    })
}

/// Distinct foreground for staged placeholders. Bold sets them apart
/// from the reversed `@mention` spans so the two source channels read
/// as different kinds at a glance.
pub fn attachment_style(kind: AttachmentKind) -> Style {
    use ratatui::style::Modifier;
    let role = match kind {
        AttachmentKind::Image => ColorRole::Warning,
        AttachmentKind::File => ColorRole::Accent,
    };
    Theme::dark_default()
        .style_for_role(role)
        .add_modifier(Modifier::BOLD)
}

/// File path carried by file-like mentions for existence checks.
/// Line ranges are already stripped by the parser, so the path can be
/// probed directly against the working directory.
pub fn mention_file_path(kind: &MentionKind) -> Option<&str> {
    match kind {
        MentionKind::File { path, .. } => Some(path.as_str()),
        MentionKind::Directory { path } => Some(path.as_str()),
        MentionKind::Skill { .. } | MentionKind::Workflow { .. } | MentionKind::Other { .. } => {
            None
        }
    }
}

/// Mention style with a validity gate. Missing files render dimmed so a
/// successful reference reads highlighted while a stale one reads muted.
pub fn mention_style_checked(kind: &MentionKind, valid: bool) -> Style {
    use ratatui::style::Modifier;
    let style = mention_style(kind);
    if valid {
        style
    } else {
        style.add_modifier(Modifier::DIM)
    }
}

/// Render `text` as spans, applying the semantic mention style to any
/// `@mention` interval while preserving `base` for the surrounding text.
/// Mention ranges come from [`parse_mentions`], so the styling stays in
/// sync with the composer and the mention panel.
pub fn mention_styled_spans(text: &str, base: Style) -> Vec<Span<'static>> {
    // Inline code and fenced code blocks carry a background fill; keep
    // their text untouched so a mention-like token inside code is not
    // mis-styled as a real reference.
    if base.bg.is_some() {
        return vec![Span::styled(text.to_string(), base)];
    }
    let mentions = parse_mentions(text);
    if mentions.is_empty() {
        return vec![Span::styled(text.to_string(), base)];
    }
    let mut out = Vec::new();
    let mut cursor = 0usize;
    for mention in mentions {
        let start = mention.range.start.min(text.len());
        let end = mention.range.end.min(text.len());
        if start > cursor {
            out.push(Span::styled(text[cursor..start].to_string(), base));
        }
        if end > start {
            out.push(Span::styled(
                text[start..end].to_string(),
                base.patch(mention_style(&mention.kind)),
            ));
        }
        cursor = cursor.max(end);
    }
    if cursor < text.len() {
        out.push(Span::styled(text[cursor..].to_string(), base));
    }
    if out.is_empty() {
        out.push(Span::styled(String::new(), base));
    }
    out
}

/// Parse all `@mention` intervals in `text`.
///
/// The scan is byte-offset based and treats every `@` followed by non-space
/// and non-`@` characters as a candidate. Existing composer content is scanned
/// left-to-right; the range covers the raw token including the `@`.
pub fn parse_mentions(text: &str) -> Vec<Mention> {
    let mut out = Vec::new();
    // Find `@<non-space>` spans. We capture until whitespace or another `@` or
    // line break. Punctuation at the end (.,;:) is trimmed as not part of the
    // mention.
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'@' {
            let start = i;
            let mut end = i + 1;
            while end < bytes.len() {
                let b = bytes[end];
                if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' || b == b'@' {
                    break;
                }
                end += 1;
            }
            if end > start + 1 {
                let mut raw: String = text[start..end].to_string();
                // Trim trailing punctuation that is not part of a file line range.
                while raw.ends_with('.')
                    || raw.ends_with(',')
                    || raw.ends_with(';')
                    || raw.ends_with(':')
                {
                    // Do not trim `:` when it precedes a line number — the raw
                    // still contains it.
                    if raw.ends_with(':') {
                        // Keep `file:10` style trailing colon is not trimmed if
                        // followed by digits earlier — but we've no digits at end
                        // here, so trim.
                        raw.pop();
                        end -= 1;
                    } else {
                        raw.pop();
                        end -= 1;
                    }
                }
                if raw.len() > 1 {
                    let token = &raw[1..];
                    let kind = classify_token(token);
                    out.push(Mention {
                        range: start..end,
                        kind,
                        raw,
                    });
                    i = end;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

/// Classify the token after `@` into a MentionKind, extracting the line range
/// for files.
fn classify_token(token: &str) -> MentionKind {
    let lower = token.to_ascii_lowercase();
    if lower.strip_prefix("skill:").is_some() {
        let name = token[6..].to_string();
        return MentionKind::Skill { name };
    }
    if lower.strip_prefix("workflow:").is_some() {
        let id = token[9..].to_string();
        return MentionKind::Workflow { id };
    }
    if lower.strip_prefix("wf:").is_some() {
        let id = token[3..].to_string();
        return MentionKind::Workflow { id };
    }
    if lower.starts_with("file:") {
        let path_part = &token[5..];
        return parse_file_token(path_part);
    }
    // No explicit prefix: decide by content.
    // If token looks like a file-range (contains `:`/`#` + digits), prefer file.
    if looks_like_file_with_lines(token) {
        return parse_file_token(token);
    }
    // If token contains `/` or `.` treat as file path.
    if token.contains('/') || token.contains('.') {
        return parse_file_token(token);
    }
    // Canonical `@kind:value` entities that do not have a dedicated
    // variant are preserved verbatim so highlighting and later resolution
    // keep their original kind.
    if let Some((kind, value)) = token.split_once(':') {
        if !kind.is_empty()
            && kind
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            return MentionKind::Other {
                kind: kind.to_string(),
                value: value.to_string(),
            };
        }
    }

    // Fallback: treat as skill when no slash/dot, then we let the caller
    // disambiguate by checking existing skill/workflow registries. The parser
    // itself keeps it as Skill for simplicity; the filter layer can reinterpret.
    // We keep it as Skill -> caller may remap to Workflow if skill not found.
    MentionKind::Skill {
        name: token.to_string(),
    }
}

fn looks_like_file_with_lines(token: &str) -> bool {
    // Contains `:` or `#` followed by digits.
    if let Some(pos) = token.find([':', '#']) {
        let rest = &token[pos + 1..];
        // skip optional '#'
        let rest = rest.strip_prefix('#').unwrap_or(rest);
        return rest.chars().next().is_some_and(|c| c.is_ascii_digit());
    }
    false
}

fn parse_file_token(token: &str) -> MentionKind {
    match split_file_lines(token) {
        Some((path, _)) if path.trim_end().ends_with('/') => MentionKind::Directory { path },
        Some((path, lines)) => MentionKind::File { path, lines },
        None => MentionKind::File {
            path: token.to_string(),
            lines: None,
        },
    }
}

/// Split `file[:#]lines` into path and optional line range.
///
/// Accepts `path`, `path:10`, `path:10-20`, `path#10`, `path#10-20`,
/// `path:#10`, `path:#10-20`. The shared contract owns the parsing so the
/// composer and the server resolve the same suffixes.
fn split_file_lines(token: &str) -> Option<(String, Option<(u32, u32)>)> {
    let (path, lines) = wf_common::mention::split_file_range(token);
    if path.is_empty() {
        return None;
    }
    Some((path, lines))
}

/// Extract the query fragment for the `@` mention that contains `cursor`.
///
/// Returns `None` when the cursor is not inside a `@query` (no preceding `@`
/// before the cursor without whitespace). The query is the substring after
/// `@` up to the cursor, not including the `@`.
pub fn mention_query_at_cursor(text: &str, cursor: usize) -> Option<String> {
    if cursor == 0 || cursor > text.len() || !text.is_char_boundary(cursor) {
        return None;
    }
    let prefix = &text[..cursor];
    // Find the last `@` before cursor.
    let at = prefix.rfind('@')?;
    let after = &prefix[at + 1..];
    // If whitespace or another `@` sits between `@` and cursor, it's not a
    // mention query.
    if after.contains(' ') || after.contains('\t') || after.contains('\n') || after.contains('@') {
        return None;
    }
    Some(after.to_string())
}

/// Fuzzy filter candidates by a case-insensitive subsequence match with a
/// relevance score (prefix / contiguous-run bonuses), aligned with the
/// fuzzysort behaviour of the opencode command palette. `query` may be empty
/// (returns all candidates in their original order).
pub fn filter_candidates(candidates: &[String], query: &str) -> Vec<String> {
    if query.is_empty() {
        return candidates.to_vec();
    }
    let needle = query.to_ascii_lowercase();
    let mut scored: Vec<(i64, &String)> = candidates
        .iter()
        .filter_map(|c| fuzzy_score(&needle, &c.to_ascii_lowercase()).map(|s| (s, c)))
        .collect();
    // Higher score first; ties keep the original order (stable sort).
    scored.sort_by_key(|item| std::cmp::Reverse(item.0));
    scored.into_iter().map(|(_, c)| c.clone()).collect()
}

/// Score `needle` against `haystack` (both already lower-cased). Returns
/// `None` when `needle` is not a subsequence of `haystack`; otherwise a
/// higher score means a better match (prefix and contiguous runs win).
pub(crate) fn fuzzy_score(needle: &str, haystack: &str) -> Option<i64> {
    if needle.is_empty() {
        return Some(0);
    }
    let n: Vec<char> = needle.chars().collect();
    let h: Vec<char> = haystack.chars().collect();
    let mut hi = 0usize;
    let mut score: i64 = 0;
    let mut prev_match: Option<usize> = None;
    let mut first_pos: Option<usize> = None;
    for &nc in &n {
        // Find the next occurrence of `nc` in the remaining haystack.
        let found = h[hi..].iter().position(|&c| c == nc).map(|p| hi + p);
        let pos = found?;
        if first_pos.is_none() {
            first_pos = Some(pos);
        }
        // Contiguous-run bonus when the previous match was adjacent.
        if let Some(prev) = prev_match {
            if pos == prev + 1 {
                score += 8;
            }
        }
        // Penalize gaps so tighter matches rank higher.
        if let Some(prev) = prev_match {
            score -= (pos - prev).saturating_sub(1) as i64 * 2;
        }
        prev_match = Some(pos);
        hi = pos + 1;
    }
    // Prefix bonus: a match that starts at the very first character.
    if first_pos == Some(0) {
        score += 16;
    }
    // Shorter haystack (more specific) wins on ties.
    score -= h.len() as i64 / 4;
    Some(score)
}

/// Scan `project_root` recursively for files, returning relative paths.
///
/// The shared contract owns the walk so the composer and the server list
/// the same candidates with the same ignore rules and bounds.
pub fn scan_files(project_root: &Path, query: Option<&str>) -> Vec<String> {
    wf_common::mention::scan_files(project_root, query)
}

/// Convenience: scan with empty query and no depth limit (tests).
pub fn scan_files_with_limit(
    project_root: &Path,
    query: Option<&str>,
    limit: usize,
) -> Vec<String> {
    let mut v = scan_files(project_root, query);
    v.truncate(limit);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_skill_and_workflow_prefixes() {
        let mentions = parse_mentions("@skill:pdf and @workflow:my-wf");
        assert_eq!(mentions.len(), 2);
        assert!(matches!(mentions[0].kind, MentionKind::Skill { .. }));
        assert!(matches!(mentions[1].kind, MentionKind::Workflow { .. }));
        assert_eq!(mentions[0].raw, "@skill:pdf");
    }

    #[test]
    fn parses_file_with_line_range_hash() {
        let mentions = parse_mentions("see @src/main.rs:#10-20 for details");
        assert_eq!(mentions.len(), 1);
        match &mentions[0].kind {
            MentionKind::File { path, lines } => {
                assert_eq!(path, "src/main.rs");
                assert_eq!(*lines, Some((10, 20)));
            }
            other => panic!("expected file, got {other:?}"),
        }
    }

    #[test]
    fn parses_file_with_colon_range() {
        let mentions = parse_mentions("@src/app/mod.rs:5-8");
        assert_eq!(mentions.len(), 1);
        match &mentions[0].kind {
            MentionKind::File { path, lines } => {
                assert_eq!(path, "src/app/mod.rs");
                assert_eq!(*lines, Some((5, 8)));
            }
            other => panic!("expected file, got {other:?}"),
        }
    }

    #[test]
    fn parses_file_without_lines_and_skill_fallback() {
        let mentions = parse_mentions("@src/main.rs @my-skill");
        assert_eq!(mentions.len(), 2);
        assert!(matches!(mentions[0].kind, MentionKind::File { .. }));
        // bare token without slash/dot is treated as skill
        assert!(matches!(mentions[1].kind, MentionKind::Skill { .. }));
    }

    #[test]
    fn parses_file_hash_without_colon() {
        let mentions = parse_mentions("@README.md#3");
        assert_eq!(mentions.len(), 1);
        match &mentions[0].kind {
            MentionKind::File { path, lines } => {
                assert_eq!(path, "README.md");
                assert_eq!(*lines, Some((3, 3)));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn parses_multiple_mentions_with_punctuation() {
        let mentions = parse_mentions("@skill:pdf, then @workflow:demo.");
        assert_eq!(mentions.len(), 2);
        // trailing punctuation trimmed
        assert_eq!(mentions[0].raw, "@skill:pdf");
        assert_eq!(mentions[1].raw, "@workflow:demo");
    }

    #[test]
    fn mention_query_at_cursor_detects_current_word() {
        let text = "hello @src/ma";
        let cursor = text.len();
        assert_eq!(
            mention_query_at_cursor(text, cursor),
            Some("src/ma".to_string())
        );
        let text2 = "hello @skill:pd";
        assert_eq!(
            mention_query_at_cursor(text2, text2.len()),
            Some("skill:pd".to_string())
        );
    }

    #[test]
    fn mention_query_none_when_outside() {
        assert_eq!(mention_query_at_cursor("hello world", 5), None);
        assert_eq!(mention_query_at_cursor("hello @world there", 18), None);
        // cursor before mention
        assert_eq!(mention_query_at_cursor("@skill:pdf", 0), None);
    }

    #[test]
    fn query_filters_by_substring_case_insensitive() {
        let cands = vec![
            "src/main.rs".to_string(),
            "src/lib.rs".to_string(),
            "README.md".to_string(),
        ];
        let filtered = filter_candidates(&cands, "main");
        assert_eq!(filtered, vec!["src/main.rs"]);
        let filtered2 = filter_candidates(&cands, "SRC");
        assert_eq!(filtered2.len(), 2);
        assert_eq!(filter_candidates(&cands, ""), cands);
    }

    #[test]
    fn file_line_split_variants() {
        assert_eq!(
            split_file_lines("a/b.rs:#10-20"),
            Some(("a/b.rs".to_string(), Some((10, 20))))
        );
        assert_eq!(
            split_file_lines("a/b.rs:10"),
            Some(("a/b.rs".to_string(), Some((10, 10))))
        );
        assert_eq!(
            split_file_lines("a/b.rs#5"),
            Some(("a/b.rs".to_string(), Some((5, 5))))
        );
        assert_eq!(
            split_file_lines("a/b.rs"),
            Some(("a/b.rs".to_string(), None))
        );
    }

    #[test]
    fn scan_files_finds_files_and_filters_query() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "fn main(){}").unwrap();
        std::fs::write(dir.path().join("src/lib.rs"), "").unwrap();
        std::fs::write(dir.path().join("README.md"), "").unwrap();
        let all = scan_files(dir.path(), None);
        assert!(all.iter().any(|p| p.contains("main.rs")));
        assert!(all.iter().any(|p| p.contains("lib.rs")));
        let filtered = scan_files(dir.path(), Some("main"));
        assert_eq!(filtered.len(), 1);
        assert!(filtered[0].contains("main.rs"));
    }

    #[test]
    fn palette_filter_narrows_like_select_list() {
        // mirrors the SelectList filter behavior tested in panels.rs
        let cands = vec![
            "/new".to_string(),
            "/model".to_string(),
            "/skills".to_string(),
        ];
        assert_eq!(filter_candidates(&cands, "mo"), vec!["/model"]);
        assert_eq!(filter_candidates(&cands, "s"), vec!["/skills"]);
    }

    #[test]
    fn tones_cover_every_kind_and_directory() {
        assert_eq!(mention_tone("file", "src/main.rs"), MentionTone::File);
        assert_eq!(mention_tone("file", "src/"), MentionTone::Directory);
        assert_eq!(mention_tone("wf", "wf-1"), MentionTone::Workflow);
        assert_eq!(mention_tone("agent_loop", "a1"), MentionTone::Agent);
        assert_eq!(mention_tone("tool", "bash"), MentionTone::Tool);
        assert_eq!(mention_tone("script", "build"), MentionTone::Script);
        assert_eq!(
            mention_tone("resource", "file:///tmp"),
            MentionTone::Resource
        );
        assert_eq!(mention_tone("unknown", "x"), MentionTone::Neutral);
    }

    #[test]
    fn role_table_is_semantic() {
        assert_eq!(
            mention_role(&MentionKind::File {
                path: "a.rs".into(),
                lines: None
            }),
            ColorRole::Accent
        );
        assert_eq!(
            mention_role(&MentionKind::Directory {
                path: "src/".into()
            }),
            ColorRole::Add
        );
        assert_eq!(
            mention_role(&MentionKind::Skill { name: "pdf".into() }),
            ColorRole::Highlight
        );
        assert_eq!(
            mention_role(&MentionKind::Workflow { id: "wf-1".into() }),
            ColorRole::Highlight
        );
        assert_eq!(
            mention_role(&MentionKind::Other {
                kind: "tool".into(),
                value: "bash".into()
            }),
            ColorRole::Warning
        );
    }

    #[test]
    fn icons_are_single_width_glyphs() {
        assert_eq!(
            mention_icon(&MentionKind::Directory {
                path: "src/".into()
            }),
            '▤'
        );
        assert_eq!(
            mention_icon(&MentionKind::File {
                path: "a.rs".into(),
                lines: None
            }),
            '▢'
        );
    }

    #[test]
    fn parses_trailing_slash_as_directory() {
        let mentions = parse_mentions("@src/ and @src/main.rs");
        assert_eq!(mentions.len(), 2);
        assert!(matches!(mentions[0].kind, MentionKind::Directory { .. }));
        assert!(matches!(mentions[1].kind, MentionKind::File { .. }));
    }

    #[test]
    fn scan_files_includes_directories() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "").unwrap();
        let all = scan_files(dir.path(), None);
        assert!(all.iter().any(|p| p == "src/"));
        assert!(all.iter().any(|p| p == "src/main.rs"));
    }
}
