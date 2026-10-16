//! Lightweight per-language code highlighting with a bounded cache.
//!
//! The tokenizer is intentionally dependency-free: it splits one source line
//! into string, comment, number and keyword spans using small per-language
//! keyword tables. It never shells out and never blocks on IO, so it stays
//! inside the per-frame budget. [`HighlightCache`] bounds memory to
//! [`HIGHLIGHT_CACHE_CAPACITY`] highlighted lines with oldest-first eviction
//! and reports byte usage for budget asserts.

use std::collections::{HashMap, VecDeque};

/// Version of the highlight backend, folded into preparation cache keys.
pub const HIGHLIGHT_VERSION: u64 = 1;

/// Maximum highlighted source lines retained by one cache.
pub const HIGHLIGHT_CACHE_CAPACITY: usize = 256;

/// Rough byte estimate for one cached highlighted line.
pub const BYTES_PER_HIGHLIGHT_LINE_ESTIMATE: usize = 512;

/// Semantic kind of one highlighted span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeTokenKind {
    Plain,
    Keyword,
    String,
    Comment,
    Number,
    Function,
    Type,
}

/// One highlighted span: source text plus its semantic kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeSpan {
    pub text: String,
    pub kind: CodeTokenKind,
}

impl CodeSpan {
    pub fn new(text: impl Into<String>, kind: CodeTokenKind) -> Self {
        Self {
            text: text.into(),
            kind,
        }
    }
}

/// Normalize a fence language tag to the tokenizer key.
pub fn normalize_language(language: Option<&str>) -> String {
    match language.unwrap_or("").trim().to_ascii_lowercase().as_str() {
        "rs" => "rust",
        "py" => "python",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "ts" | "tsx" => "typescript",
        "sh" | "bash" | "zsh" => "bash",
        "yml" => "yaml",
        "dockerfile" => "dockerfile",
        "c++" | "cc" | "hpp" => "cpp",
        "c#" => "csharp",
        other => other,
    }
    .to_string()
}

/// Comment prefix for line comments in `language`.
fn line_comment(language: &str) -> Option<&'static str> {
    Some(match language {
        "rust" | "javascript" | "typescript" | "go" | "java" | "cpp" | "c" | "csharp" => "//",
        "python" | "bash" | "toml" | "yaml" | "dockerfile" => "#",
        "lua" | "sql" => "--",
        _ => return None,
    })
}

/// Whether `word` is a keyword in `language`.
fn is_keyword(language: &str, word: &str) -> bool {
    let specific = match language {
        "rust" => RUST_KEYWORDS.as_slice(),
        "python" => PYTHON_KEYWORDS.as_slice(),
        "javascript" | "typescript" => JS_KEYWORDS.as_slice(),
        "bash" => SHELL_KEYWORDS.as_slice(),
        _ => &[],
    };
    specific.contains(&word) || GENERIC_KEYWORDS.contains(&word)
}

const RUST_KEYWORDS: [&str; 52] = [
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "union",
    "unsafe", "use", "where", "while", "Ok", "Err", "Some", "None", "Vec", "String", "Option",
    "Result", "Box", "Arc", "u8", "u16", "u32",
];

const PYTHON_KEYWORDS: [&str; 32] = [
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class",
    "continue", "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if",
    "import", "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "while",
];

const JS_KEYWORDS: [&str; 32] = [
    "await", "break", "case", "catch", "class", "const", "continue", "debugger", "default",
    "delete", "do", "else", "export", "extends", "false", "finally", "for", "function", "if",
    "import", "in", "instanceof", "new", "null", "return", "super", "switch", "this", "throw",
    "true", "try", "typeof",
];

const SHELL_KEYWORDS: [&str; 16] = [
    "if", "then", "else", "elif", "fi", "for", "while", "do", "done", "case", "esac", "function",
    "return", "exit", "export", "local",
];

const GENERIC_KEYWORDS: [&str; 12] = [
    "if", "else", "for", "while", "return", "true", "false", "null", "None", "class", "def", "fn",
];

/// Whether `word` looks like a type name (leading capital or known builtin).
fn is_type_name(word: &str) -> bool {
    word.chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
}

/// Tokenize one source line without any cache lookup. Iterates characters so
/// multibyte text passes through untouched; only ASCII structure characters
/// steer the tokenizer.
pub fn highlight_line(language: Option<&str>, line: &str) -> Vec<CodeSpan> {
    let language = normalize_language(language);
    if language.is_empty() {
        return vec![CodeSpan::new(line, CodeTokenKind::Plain)];
    }
    if let Some(prefix) = line_comment(language.as_str()) {
        if line.trim_start().starts_with(prefix) {
            return vec![CodeSpan::new(line, CodeTokenKind::Comment)];
        }
    }
    let mut spans = Vec::new();
    let mut plain = String::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '"' || ch == '\'' || ch == '`' {
            if !plain.is_empty() {
                spans.push(CodeSpan::new(std::mem::take(&mut plain), CodeTokenKind::Plain));
            }
            let mut text = String::new();
            text.push(ch);
            i += 1;
            while i < chars.len() {
                let next = chars[i];
                text.push(next);
                i += 1;
                if next == '\\' && i < chars.len() {
                    text.push(chars[i]);
                    i += 1;
                    continue;
                }
                if next == ch {
                    break;
                }
            }
            spans.push(CodeSpan::new(text, CodeTokenKind::String));
            continue;
        }
        if let Some(prefix) = line_comment(language.as_str()) {
            let rest: String = chars[i..].iter().collect();
            if rest.starts_with(prefix) {
                if !plain.is_empty() {
                    spans.push(CodeSpan::new(std::mem::take(&mut plain), CodeTokenKind::Plain));
                }
                spans.push(CodeSpan::new(rest, CodeTokenKind::Comment));
                break;
            }
        }
        if ch.is_ascii_digit() {
            if !plain.is_empty() {
                spans.push(CodeSpan::new(std::mem::take(&mut plain), CodeTokenKind::Plain));
            }
            let mut text = String::new();
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '.')
            {
                text.push(chars[i]);
                i += 1;
            }
            spans.push(CodeSpan::new(text, CodeTokenKind::Number));
            continue;
        }
        if ch.is_ascii_alphabetic() || ch == '_' {
            let mut word = String::new();
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                word.push(chars[i]);
                i += 1;
            }
            let rest: String = chars[i..].iter().collect();
            let follows_call = rest.trim_start().starts_with('(');
            if !plain.is_empty() {
                spans.push(CodeSpan::new(std::mem::take(&mut plain), CodeTokenKind::Plain));
            }
            if is_keyword(language.as_str(), &word) {
                spans.push(CodeSpan::new(word, CodeTokenKind::Keyword));
            } else if follows_call {
                spans.push(CodeSpan::new(word, CodeTokenKind::Function));
            } else if is_type_name(&word) {
                spans.push(CodeSpan::new(word, CodeTokenKind::Type));
            } else {
                spans.push(CodeSpan::new(word, CodeTokenKind::Plain));
            }
            continue;
        }
        plain.push(ch);
        i += 1;
    }
    if !plain.is_empty() {
        spans.push(CodeSpan::new(std::mem::take(&mut plain), CodeTokenKind::Plain));
    }
    if spans.is_empty() {
        spans.push(CodeSpan::new(line, CodeTokenKind::Plain));
    }
    spans
}

/// Bounded cache of highlighted source lines keyed by language plus content.
#[derive(Debug, Clone, Default)]
pub struct HighlightCache {
    entries: HashMap<String, Vec<CodeSpan>>,
    order: VecDeque<String>,
    hits: u64,
    misses: u64,
}

impl HighlightCache {
    /// Empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Highlight one line, reusing the cached spans on a hit.
    pub fn highlight(&mut self, language: Option<&str>, line: &str) -> Vec<CodeSpan> {
        let key = cache_key(language, line);
        if let Some(cached) = self.entries.get(&key) {
            self.hits += 1;
            return cached.clone();
        }
        self.misses += 1;
        let spans = highlight_line(language, line);
        if self.entries.len() >= HIGHLIGHT_CACHE_CAPACITY && !self.entries.contains_key(&key) {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            }
        }
        self.order.push_back(key.clone());
        self.entries.insert(key, spans.clone());
        spans
    }

    /// Cached line count.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache holds nothing.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Estimated cached bytes for budget asserts.
    pub fn bytes(&self) -> usize {
        self.entries.len()
            .saturating_mul(BYTES_PER_HIGHLIGHT_LINE_ESTIMATE)
    }

    /// Cache hits since creation.
    pub fn hits(&self) -> u64 {
        self.hits
    }

    /// Cache misses since creation.
    pub fn misses(&self) -> u64 {
        self.misses
    }
}

/// Cache key combining the normalized language with the source line.
fn cache_key(language: Option<&str>, line: &str) -> String {
    let mut key = String::with_capacity(line.len() + 16);
    key.push_str(&normalize_language(language));
    key.push('\u{1f}');
    key.push_str(line);
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_keywords_functions_and_strings_classify() {
        let spans = highlight_line(Some("rust"), "fn main() { let s = \"hi\"; }");
        assert!(spans
            .iter()
            .any(|span| span.text == "fn" && span.kind == CodeTokenKind::Keyword));
        assert!(spans
            .iter()
            .any(|span| span.text == "main" && span.kind == CodeTokenKind::Function));
        assert!(spans
            .iter()
            .any(|span| span.text == "\"hi\"" && span.kind == CodeTokenKind::String));
    }

    #[test]
    fn comments_and_numbers_classify() {
        let spans = highlight_line(Some("rust"), "// a comment");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].kind, CodeTokenKind::Comment);
        let spans = highlight_line(Some("python"), "x = 42  # done");
        assert!(spans
            .iter()
            .any(|span| span.kind == CodeTokenKind::Number));
        assert!(spans
            .iter()
            .any(|span| span.kind == CodeTokenKind::Comment));
    }

    #[test]
    fn aliases_normalize_to_canonical_languages() {
        assert_eq!(normalize_language(Some("rs")), "rust");
        assert_eq!(normalize_language(Some("  TS ")), "typescript");
        assert_eq!(normalize_language(None), "");
    }

    #[test]
    fn cache_reuses_spans_and_bounds_memory() {
        let mut cache = HighlightCache::new();
        let first = cache.highlight(Some("rust"), "let x = 1;");
        let second = cache.highlight(Some("rust"), "let x = 1;");
        assert_eq!(first, second);
        assert_eq!(cache.hits(), 1);
        for i in 0..(HIGHLIGHT_CACHE_CAPACITY + 50) {
            cache.highlight(Some("rust"), &format!("let var_{i} = {i};"));
        }
        assert!(cache.len() <= HIGHLIGHT_CACHE_CAPACITY);
        assert!(cache.bytes() <= HIGHLIGHT_CACHE_CAPACITY * BYTES_PER_HIGHLIGHT_LINE_ESTIMATE);
    }
}
