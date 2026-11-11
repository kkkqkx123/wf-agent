//! Unified tool output overflow policy: bounded inline prefixes with
//! full-text spill files readable on demand.
//!
//! Executors whose output can exceed model-friendly sizes route long text
//! through [`truncate_with_spill`]: the inline result keeps a line- and
//! byte-bounded head prefix plus a pointer marker, while the complete text
//! is written to a spill file the model can read back with the file tools.
//! Spilling is best effort: an unwritable directory degrades to plain
//! truncation and never fails the tool call. Already paged tools (file
//! reads, shell session windows) stay exempt by construction: they never
//! call this module.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Maximum inline lines kept from an overflowing output.
pub const MAX_OUTPUT_LINES: usize = 2000;
/// Maximum inline bytes kept from an overflowing output.
pub const MAX_OUTPUT_BYTES: usize = 50 * 1024;

/// Message metadata key marking a truncated tool result.
pub const TRUNCATED_METADATA_KEY: &str = "truncated";
/// Message metadata key carrying the spilled full-output path.
pub const OUTPUT_PATH_METADATA_KEY: &str = "output_path";

/// Subdirectory (under the volatile tmp root) holding spilled outputs.
pub const SPILL_SUBDIR: &str = "tool-output";

static SPILL_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Outcome of [`truncate_with_spill`].
pub struct SpillOutcome {
    /// Inline text: the original when within limits, otherwise the bounded
    /// prefix plus a pointer marker.
    pub kept_text: String,
    /// Spill file holding the full text. `None` when within limits or when
    /// spilling failed (plain truncation).
    pub spilled_path: Option<PathBuf>,
    /// Total line count of the full text.
    pub total_lines: usize,
}

/// Resolve the spill directory: the execution workdir's volatile tmp tree
/// when known (readable back through the file tools under the same scope),
/// otherwise the process temp directory.
pub fn resolve_spill_dir(workdir: Option<&str>) -> PathBuf {
    match workdir {
        Some(root) => Path::new(root).join(".wf").join("tmp").join(SPILL_SUBDIR),
        None => std::env::temp_dir().join("wf-tool-output"),
    }
}

/// Truncate `text` to the unified limits, spilling the full text to
/// `spill_dir` when over either limit.
pub fn truncate_with_spill(text: &str, spill_dir: &Path) -> SpillOutcome {
    let total_lines = text.lines().count().max(1);
    if text.lines().count() <= MAX_OUTPUT_LINES && text.len() <= MAX_OUTPUT_BYTES {
        return SpillOutcome {
            kept_text: text.to_string(),
            spilled_path: None,
            total_lines,
        };
    }

    let mut kept = String::new();
    let mut kept_lines = 0usize;
    for line in text.lines() {
        if kept_lines >= MAX_OUTPUT_LINES || kept.len() + line.len() + 1 > MAX_OUTPUT_BYTES {
            break;
        }
        if kept_lines > 0 {
            kept.push('\n');
        }
        kept.push_str(line);
        kept_lines += 1;
    }

    let spilled_path = spill_full_text(text, spill_dir);
    let kept_text = format!(
        "{kept}\n{}",
        spill_marker(kept_lines, total_lines, spilled_path.as_deref())
    );
    SpillOutcome {
        kept_text,
        spilled_path,
        total_lines,
    }
}

/// Pointer marker appended to a truncated prefix.
pub fn spill_marker(kept_lines: usize, total_lines: usize, path: Option<&Path>) -> String {
    match path {
        Some(path) => format!(
            "[showing lines 1-{kept_lines} of {total_lines}; full output saved to {}]",
            path.display()
        ),
        None => {
            format!("[showing lines 1-{kept_lines} of {total_lines}; full output not retained]")
        }
    }
}

fn spill_full_text(text: &str, spill_dir: &Path) -> Option<PathBuf> {
    if std::fs::create_dir_all(spill_dir).is_err() {
        return None;
    }
    let id = SPILL_COUNTER.fetch_add(1, Ordering::Relaxed);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let name = format!(
        "tool_{:012x}.txt",
        (stamp.wrapping_add(id as u128)) & 0xffff_ffff_ffff
    );
    let path = spill_dir.join(name);
    std::fs::write(&path, text).ok()?;
    Some(path)
}

/// Copy overflow metadata onto an execution result.
pub fn apply_overflow_metadata(
    result: &mut wf_types::tool::ToolExecutionResult,
    outcome: &SpillOutcome,
) {
    if outcome.spilled_path.is_none()
        && outcome.kept_text.len() <= MAX_OUTPUT_BYTES
        && outcome.total_lines <= MAX_OUTPUT_LINES
    {
        return;
    }
    result.truncated = Some(true);
    result.output_path = outcome
        .spilled_path
        .as_ref()
        .map(|p| p.to_string_lossy().to_string());
}

/// Attach overflow metadata to a tool result message.
pub fn apply_overflow_message_metadata(
    metadata: &mut Option<wf_types::Metadata>,
    result: &wf_types::tool::ToolExecutionResult,
) {
    if result.truncated != Some(true) {
        return;
    }
    let map = metadata.get_or_insert_with(Default::default);
    map.insert(
        TRUNCATED_METADATA_KEY.to_string(),
        serde_json::Value::Bool(true),
    );
    if let Some(path) = result.output_path.as_deref() {
        map.insert(
            OUTPUT_PATH_METADATA_KEY.to_string(),
            serde_json::Value::String(path.to_string()),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_passes_through_without_spill() {
        let dir = std::env::temp_dir().join("wf-spill-test-passthrough");
        let outcome = truncate_with_spill("hello\nworld", &dir);
        assert_eq!(outcome.kept_text, "hello\nworld");
        assert!(outcome.spilled_path.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn long_text_spills_and_keeps_pointer() {
        let dir = std::env::temp_dir().join("wf-spill-test-overflow");
        let _ = std::fs::remove_dir_all(&dir);
        let text = "line\n".repeat(5000);
        let outcome = truncate_with_spill(&text, &dir);
        let path = outcome.spilled_path.expect("overflow must spill");
        assert!(outcome.kept_text.contains("full output saved to"));
        assert!(outcome.kept_text.contains(&path.display().to_string()));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn byte_limit_triggers_before_line_limit() {
        let dir = std::env::temp_dir().join("wf-spill-test-bytes");
        let _ = std::fs::remove_dir_all(&dir);
        let text = "x".repeat(MAX_OUTPUT_BYTES + 100);
        let outcome = truncate_with_spill(&text, &dir);
        assert!(outcome.spilled_path.is_some());
        assert!(outcome.kept_text.len() < text.len());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
