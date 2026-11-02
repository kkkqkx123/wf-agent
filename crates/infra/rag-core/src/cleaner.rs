//! Document cleaning: whitespace, repetition and noise removal.
//!
//! Cleaning runs before chunking on the ingest path. Every rule is
//! switchable through [`CleanerConfig`] so callers can preserve verbatim
//! text when the source is already normalized.

use serde::{Deserialize, Serialize};

/// Switchable cleaning rules applied in a fixed order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CleanerConfig {
    /// Trim leading and trailing whitespace of the whole document.
    pub trim: bool,
    /// Collapse runs of whitespace inside lines to single spaces.
    pub collapse_whitespace: bool,
    /// Drop lines that are empty after trimming.
    pub remove_empty_lines: bool,
    /// Drop consecutive duplicate lines, keeping the first occurrence.
    pub dedup_consecutive_lines: bool,
    /// Drop lines shorter than this length after trimming (0 disables).
    pub min_line_len: usize,
}

impl Default for CleanerConfig {
    fn default() -> Self {
        Self {
            trim: true,
            collapse_whitespace: true,
            remove_empty_lines: true,
            dedup_consecutive_lines: false,
            min_line_len: 0,
        }
    }
}

impl CleanerConfig {
    /// Rules disabled: input passes through unchanged.
    pub fn disabled() -> Self {
        Self {
            trim: false,
            collapse_whitespace: false,
            remove_empty_lines: false,
            dedup_consecutive_lines: false,
            min_line_len: 0,
        }
    }

    /// Whether any rule is active.
    pub fn is_enabled(&self) -> bool {
        self.trim
            || self.collapse_whitespace
            || self.remove_empty_lines
            || self.dedup_consecutive_lines
            || self.min_line_len > 0
    }
}

/// Applies the configured rules to document text.
pub fn clean_text(content: &str, config: &CleanerConfig) -> String {
    if !config.is_enabled() {
        return content.to_string();
    }
    let mut lines: Vec<String> = content.lines().map(str::to_string).collect();
    if config.collapse_whitespace {
        for line in &mut lines {
            *line = collapse_line(line);
        }
    } else {
        for line in &mut lines {
            *line = line.trim().to_string();
        }
    }
    if config.remove_empty_lines {
        lines.retain(|line| !line.trim().is_empty());
    }
    if config.min_line_len > 0 {
        lines.retain(|line| line.trim().chars().count() >= config.min_line_len);
    }
    if config.dedup_consecutive_lines {
        let mut kept: Vec<String> = Vec::with_capacity(lines.len());
        for line in lines {
            if kept.last() != Some(&line) {
                kept.push(line);
            }
        }
        lines = kept;
    }
    let mut cleaned = lines.join("\n");
    if config.trim {
        cleaned = cleaned.trim().to_string();
    }
    cleaned
}

fn collapse_line(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_space = false;
    for ch in line.chars() {
        if ch.is_whitespace() {
            if !in_space {
                out.push(' ');
            }
            in_space = true;
        } else {
            out.push(ch);
            in_space = false;
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_config_passes_through() {
        let input = "  a  b  \n\n  c  ";
        assert_eq!(clean_text(input, &CleanerConfig::disabled()), input);
    }

    #[test]
    fn default_config_normalizes_whitespace() {
        let cleaned = clean_text("  hello   world  \n\n\n  foo  ", &CleanerConfig::default());
        assert_eq!(cleaned, "hello world\nfoo");
    }

    #[test]
    fn dedup_removes_consecutive_repeats() {
        let config = CleanerConfig {
            dedup_consecutive_lines: true,
            ..CleanerConfig::default()
        };
        let cleaned = clean_text("a\na\nb", &config);
        assert_eq!(cleaned, "a\nb");
    }

    #[test]
    fn short_lines_dropped_by_threshold() {
        let config = CleanerConfig {
            min_line_len: 3,
            ..CleanerConfig::default()
        };
        let cleaned = clean_text("ok\nfine\nx", &config);
        assert!(!cleaned.lines().any(|line| line.trim() == "x"));
        assert!(cleaned.contains("fine"));
    }
}
