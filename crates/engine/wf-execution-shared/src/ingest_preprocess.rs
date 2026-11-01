//! Pre-history ingest shaping: tool output distillation and file-read dedup helpers.
//!
//! This module owns everything that rewrites a tool result before it enters
//! history: classification by tool name, length-driven distillation and the
//! archive-pointer vocabulary for deduplicated file reads. Compaction keeps
//! only budget, plan and block bookkeeping and never touches raw tool text.

/// Metadata key carrying an archive pointer on a distilled message.
pub const ARCHIVE_REF_KEY: &str = "archive_ref";
/// Archive pointer prefix: `a:<blob-id>`.
pub const ARCHIVE_REF_PREFIX: &str = "a:";

/// Tool output class driving pre-history distillation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolOutputKind {
    Test,
    Search,
    Diff,
    File,
    Command,
}

/// Classify a tool result by tool name for distillation.
pub fn classify_tool(tool_name: &str) -> ToolOutputKind {
    let name = tool_name.to_lowercase();
    if name == "read_file" || name == "read-file" {
        return ToolOutputKind::File;
    }
    if name.contains("test") || name.contains("build") {
        return ToolOutputKind::Test;
    }
    if name.contains("search") || name.contains("grep") || name.contains("code_") {
        return ToolOutputKind::Search;
    }
    if name.contains("diff") {
        return ToolOutputKind::Diff;
    }
    ToolOutputKind::Command
}

fn take_head_tail(text: &str, head: usize, tail: usize) -> String {
    if text.len() <= head.saturating_add(tail).saturating_add(128) {
        return text.to_string();
    }
    let head_part: String = text.chars().take(head).collect();
    let tail_part: String = text
        .chars()
        .rev()
        .take(tail)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!(
        "{head_part}\n…[truncated {0} chars]…\n{tail_part}",
        text.len()
    )
}

fn important_lines<'a>(lines: &[&'a str]) -> Vec<&'a str> {
    lines
        .iter()
        .copied()
        .filter(|line| {
            let lower = line.to_lowercase();
            lower.contains("error")
                || lower.contains("fail")
                || lower.contains("panic")
                || lower.contains("assert")
                || lower.contains("traceback")
                || line.contains(".rs:")
                || line.contains(".ts:")
                || line.contains(".py:")
        })
        .collect()
}

/// Distill one tool result before it enters history: keep conclusions,
/// errors and file references, drop bulk output. Returns the replacement
/// text; the original is archived by the caller.
pub fn distill_tool_output(kind: ToolOutputKind, text: &str) -> String {
    if text.len() < 2000 {
        return text.to_string();
    }
    let lines: Vec<&str> = text.lines().collect();
    match kind {
        ToolOutputKind::File => text.to_string(),
        ToolOutputKind::Test => {
            let hits = important_lines(&lines);
            let kept: Vec<&str> = hits.into_iter().take(60).collect();
            let tail: Vec<&str> = lines
                .iter()
                .rev()
                .take(25)
                .copied()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            format!(
                "[distilled test output: {0} lines -> {1} kept]\n{2}\n--- tail ---\n{3}",
                lines.len(),
                kept.len(),
                kept.join("\n"),
                tail.join("\n")
            )
        }
        ToolOutputKind::Search => {
            let kept: Vec<&str> = lines.iter().take(40).copied().collect();
            format!(
                "[distilled search output: {0} hits]\n{1}",
                lines.len(),
                kept.join("\n")
            )
        }
        ToolOutputKind::Diff => {
            let files: Vec<&&str> = lines
                .iter()
                .filter(|l| l.starts_with("diff --git") || l.starts_with("+++ b/"))
                .take(64)
                .collect();
            let added = lines.iter().filter(|l| l.starts_with('+')).count();
            let removed = lines.iter().filter(|l| l.starts_with('-')).count();
            format!(
                "[distilled diff: {0} files, +{1}/-{2}]\n{3}\n{4}",
                files.len(),
                added,
                removed,
                files
                    .into_iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                    .join("\n"),
                take_head_tail(text, 1500, 1000)
            )
        }
        ToolOutputKind::Command => take_head_tail(text, 3000, 1500),
    }
}

/// Content hash for file-read deduplication.
pub fn blake3_hex(text: &str) -> String {
    blake3::hash(text.as_bytes()).to_hex().to_string()
}

/// Pointer text replacing a repeated file read.
pub fn file_pointer_text(path: &str, commit: &str, seq: u64) -> String {
    if commit.is_empty() {
        format!("[file {path} already read, see m{seq}]")
    } else {
        format!("[file {path} @ {commit} already read, see m{seq}]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_tool_output_passes_through() {
        assert_eq!(
            distill_tool_output(ToolOutputKind::Command, "ok"),
            "ok".to_string()
        );
    }

    #[test]
    fn long_command_output_truncates() {
        let text = "x\n".repeat(5000);
        let out = distill_tool_output(ToolOutputKind::Command, &text);
        assert!(out.len() < text.len());
        assert!(out.contains("truncated"));
    }
}
