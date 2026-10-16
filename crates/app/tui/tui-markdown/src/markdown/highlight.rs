//! Lightweight per-language code highlighting with a bounded cache.
//!
//! The tokenizer is intentionally dependency-free: it splits one source line
//! into string, comment, number and keyword spans using small per-language
//! keyword tables. It never shells out and never blocks on IO, so it stays
//! inside the per-frame budget. [`HighlightCache`] bounds memory to
//! [`HIGHLIGHT_CACHE_CAPACITY`] highlighted lines with oldest-first eviction
//! and reports byte usage for budget asserts.
//!
//! Scope note: this backend tokenizes lexical structure only (keywords,
//! strings, comments, numbers, function and type shapes). It does not do
//! scope-accurate coloring: nested scopes, semantic types and theme-aware
//! symbol resolution are out of scope. A future external backend (for example
//! a TextMate-style engine) may replace the tokenizer while keeping the same
//! cache key, memory profile and version-bump contract below.

use std::collections::{HashMap, VecDeque};

/// Version of the highlight backend, folded into preparation cache keys.
/// Bumped when the cache key format changed from language-plus-text to
/// language-plus-content-hash, and must bump again whenever the backend
/// implementation switches so stale laid-out rows never survive.
pub const HIGHLIGHT_VERSION: u64 = 2;

/// Identity of the active highlight backend. Switching backends must bump
/// [`HIGHLIGHT_VERSION`] so row and preparation caches invalidate.
pub const HIGHLIGHT_BACKEND: u64 = 1;

/// Human name of the active backend for diagnostics.
pub const HIGHLIGHT_BACKEND_NAME: &str = "tokenizer";

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
        "py" | "py3" | "python3" => "python",
        "js" | "jsx" | "mjs" | "cjs" | "node" => "javascript",
        "ts" | "tsx" | "mts" | "cts" => "typescript",
        "sh" | "bash" | "zsh" | "shell" | "ps1" => "bash",
        "yml" => "yaml",
        "dockerfile" => "dockerfile",
        "c++" | "cc" | "cxx" | "hpp" | "h" | "cpp" => "cpp",
        "c#" | "cs" => "csharp",
        "golang" => "go",
        "rb" => "ruby",
        "kt" | "kts" => "kotlin",
        "hs" => "haskell",
        "md" | "markdown" => "markdown",
        other => other,
    }
    .to_string()
}

/// Comment prefix for line comments in `language`.
fn line_comment(language: &str) -> Option<&'static str> {
    Some(match language {
        "rust" | "javascript" | "typescript" | "go" | "java" | "cpp" | "c" | "csharp" | "swift"
        | "kotlin" | "scala" | "php" | "css" => "//",
        "python" | "bash" | "toml" | "yaml" | "dockerfile" | "ruby" | "powershell" => "#",
        "lua" | "sql" | "haskell" => "--",
        "html" | "xml" | "markdown" => "<!--",
        _ => return None,
    })
}

/// Block comment delimiters for `language`, if the language has any.
fn block_comment_delims(language: &str) -> Option<(&'static str, &'static str)> {
    Some(match language {
        "rust" | "javascript" | "typescript" | "go" | "java" | "cpp" | "c" | "csharp" | "swift"
        | "kotlin" | "scala" | "php" | "css" => ("/*", "*/"),
        "lua" | "sql" => ("--[[", "]]"),
        "html" | "xml" | "markdown" => ("<!--", "-->"),
        "python" | "bash" | "ruby" => ("", ""),
        _ => return None,
    })
    .filter(|(open, _)| !open.is_empty())
}

/// Whether `word` is a keyword in `language`.
fn is_keyword(language: &str, word: &str) -> bool {
    let specific = match language {
        "rust" => RUST_KEYWORDS.as_slice(),
        "python" => PYTHON_KEYWORDS.as_slice(),
        "javascript" | "typescript" => JS_KEYWORDS.as_slice(),
        "bash" => SHELL_KEYWORDS.as_slice(),
        "go" => GO_KEYWORDS.as_slice(),
        "java" => JAVA_KEYWORDS.as_slice(),
        "cpp" | "c" => CPP_KEYWORDS.as_slice(),
        "csharp" => CSHARP_KEYWORDS.as_slice(),
        "ruby" => RUBY_KEYWORDS.as_slice(),
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
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue",
    "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import",
    "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "while",
];

const JS_KEYWORDS: [&str; 32] = [
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "import",
    "in",
    "instanceof",
    "new",
    "null",
    "return",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
];

const SHELL_KEYWORDS: [&str; 16] = [
    "if", "then", "else", "elif", "fi", "for", "while", "do", "done", "case", "esac", "function",
    "return", "exit", "export", "local",
];

const GO_KEYWORDS: [&str; 25] = [
    "break",
    "case",
    "chan",
    "const",
    "continue",
    "default",
    "defer",
    "else",
    "fallthrough",
    "for",
    "func",
    "go",
    "goto",
    "if",
    "import",
    "interface",
    "map",
    "package",
    "range",
    "return",
    "select",
    "struct",
    "switch",
    "type",
    "var",
];

const JAVA_KEYWORDS: [&str; 32] = [
    "abstract",
    "assert",
    "boolean",
    "break",
    "byte",
    "case",
    "catch",
    "char",
    "class",
    "continue",
    "default",
    "do",
    "double",
    "else",
    "enum",
    "extends",
    "final",
    "finally",
    "float",
    "for",
    "if",
    "implements",
    "import",
    "instanceof",
    "int",
    "new",
    "package",
    "return",
    "switch",
    "this",
    "throw",
    "void",
];

const CPP_KEYWORDS: [&str; 32] = [
    "alignas",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "constexpr",
    "continue",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "explicit",
    "export",
    "for",
    "if",
    "namespace",
    "new",
    "noexcept",
    "private",
    "public",
    "return",
    "struct",
    "switch",
    "template",
    "this",
    "throw",
    "try",
    "typename",
    "using",
    "virtual",
];

const CSHARP_KEYWORDS: [&str; 24] = [
    "abstract",
    "as",
    "base",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "do",
    "else",
    "enum",
    "finally",
    "for",
    "if",
    "namespace",
    "new",
    "return",
    "switch",
    "this",
    "throw",
    "using",
    "var",
    "void",
];

const RUBY_KEYWORDS: [&str; 24] = [
    "alias", "and", "begin", "break", "case", "class", "def", "do", "else", "end", "false", "for",
    "if", "in", "module", "next", "nil", "not", "or", "return", "self", "true", "while", "yield",
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

/// 64-bit FNV-1a hash over bytes, used for content hashing without deps.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Content hash of one source line for cache keys. Hashing avoids retaining
/// the full line text in the key, so oversize lines cost a fixed 8 bytes.
pub fn content_hash(line: &str) -> u64 {
    fnv1a(line.as_bytes())
}

/// Multiline string delimiter carried across lines in one code block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MultilineDelim {
    TripleDouble,
    TripleSingle,
    LuaBracket,
    Backtick,
}

impl MultilineDelim {
    fn open(self) -> &'static str {
        match self {
            MultilineDelim::TripleDouble => "\"\"\"",
            MultilineDelim::TripleSingle => "'''",
            MultilineDelim::LuaBracket => "[[",
            MultilineDelim::Backtick => "`",
        }
    }

    fn close(self) -> &'static str {
        self.open()
    }

    fn discriminant(self) -> u8 {
        match self {
            MultilineDelim::TripleDouble => 1,
            MultilineDelim::TripleSingle => 2,
            MultilineDelim::LuaBracket => 3,
            MultilineDelim::Backtick => 4,
        }
    }
}

/// Block-level tokenizer state threaded through one fenced code block.
/// Tracks an unclosed block comment and an unclosed multiline string so a
/// comment or string spanning lines highlights as one construct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HighlightBlockState {
    in_block_comment: bool,
    open_string: Option<MultilineDelim>,
}

impl HighlightBlockState {
    /// Empty state for the first line of a block.
    pub fn new() -> Self {
        Self::default()
    }

    fn state_tag(self) -> u8 {
        let base = if self.in_block_comment { 16 } else { 0 };
        base + self.open_string.map(|d| d.discriminant()).unwrap_or(0)
    }
}

/// Tokenize one source line without any cache lookup. Iterates characters so
/// multibyte text passes through untouched; only ASCII structure characters
/// steer the tokenizer. Single-line block comments (`/* ... */`) highlight
/// inline; an unclosed opener highlights to end of line (the block-aware
/// entry point carries the continuation to the next line).
pub fn highlight_line(language: Option<&str>, line: &str) -> Vec<CodeSpan> {
    let mut state = HighlightBlockState::new();
    highlight_line_with_state(language, line, &mut state)
}

/// Tokenize one line under an incoming block state, updating the state for
/// the next line. The cache-aware wrapper mixes the incoming state into the
/// key so identical text in different block contexts never collides.
pub fn highlight_line_with_state(
    language: Option<&str>,
    line: &str,
    state: &mut HighlightBlockState,
) -> Vec<CodeSpan> {
    let language = normalize_language(language);
    if language.is_empty() {
        return vec![CodeSpan::new(line, CodeTokenKind::Plain)];
    }
    if state.in_block_comment {
        return consume_block_continuation(language.as_str(), line, state);
    }
    if let Some(delim) = state.open_string {
        return consume_string_continuation(language.as_str(), line, state, delim);
    }
    if let Some(prefix) = line_comment(language.as_str()) {
        if line.trim_start().starts_with(prefix) {
            return vec![CodeSpan::new(line, CodeTokenKind::Comment)];
        }
    }
    tokenize_fresh(language.as_str(), line, state)
}

/// Consume a line that starts inside a block comment. Text up to the close
/// delimiter stays a comment; the remainder tokenizes fresh.
fn consume_block_continuation(
    language: &str,
    line: &str,
    state: &mut HighlightBlockState,
) -> Vec<CodeSpan> {
    let close = block_comment_delims(language)
        .map(|(_, c)| c)
        .unwrap_or("*/");
    if let Some(end) = line.find(close) {
        let split = end + close.len();
        let (head, tail) = line.split_at(split);
        state.in_block_comment = false;
        let mut spans = vec![CodeSpan::new(head, CodeTokenKind::Comment)];
        if !tail.is_empty() {
            spans.extend(tokenize_fresh(language, tail, state));
        }
        spans
    } else {
        vec![CodeSpan::new(line, CodeTokenKind::Comment)]
    }
}

/// Consume a line that starts inside a multiline string. Text up to the close
/// delimiter stays a string; the remainder tokenizes fresh.
fn consume_string_continuation(
    language: &str,
    line: &str,
    state: &mut HighlightBlockState,
    delim: MultilineDelim,
) -> Vec<CodeSpan> {
    let close = delim.close();
    if let Some(end) = line.find(close) {
        let split = end + close.len();
        let (head, tail) = line.split_at(split);
        state.open_string = None;
        let mut spans = vec![CodeSpan::new(head, CodeTokenKind::String)];
        if !tail.is_empty() {
            spans.extend(tokenize_fresh(language, tail, state));
        }
        spans
    } else {
        vec![CodeSpan::new(line, CodeTokenKind::String)]
    }
}

/// Tokenize a line known to start outside any block construct. May set the
/// outgoing block comment or multiline string state when an opener does not
/// close on the same line.
fn tokenize_fresh(language: &str, line: &str, state: &mut HighlightBlockState) -> Vec<CodeSpan> {
    let mut spans = Vec::new();
    let mut plain = String::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        if let Some((open, close)) = block_comment_delims(language) {
            let rest: String = chars[i..].iter().collect();
            if rest.starts_with(open) {
                if let Some(end) = rest.find(close) {
                    if !plain.is_empty() {
                        spans.push(CodeSpan::new(
                            std::mem::take(&mut plain),
                            CodeTokenKind::Plain,
                        ));
                    }
                    let text = rest[..end + close.len()].to_string();
                    spans.push(CodeSpan::new(text, CodeTokenKind::Comment));
                    i += rest[..end + close.len()].chars().count();
                    continue;
                }
                if !plain.is_empty() {
                    spans.push(CodeSpan::new(
                        std::mem::take(&mut plain),
                        CodeTokenKind::Plain,
                    ));
                }
                spans.push(CodeSpan::new(rest, CodeTokenKind::Comment));
                state.in_block_comment = true;
                break;
            }
        }
        if starts_multiline_open(language, &chars[i..]) {
            if let Some(delim) = match_multiline_open(&chars[i..]) {
                let rest: String = chars[i..].iter().collect();
                let open = delim.open();
                let after_open = &rest[open.len()..];
                if let Some(end) = after_open.find(open) {
                    if !plain.is_empty() {
                        spans.push(CodeSpan::new(
                            std::mem::take(&mut plain),
                            CodeTokenKind::Plain,
                        ));
                    }
                    let text = format!("{open}{}{open}", &after_open[..end]);
                    spans.push(CodeSpan::new(text, CodeTokenKind::String));
                    i += open.len() + end + open.len();
                    continue;
                }
                if !plain.is_empty() {
                    spans.push(CodeSpan::new(
                        std::mem::take(&mut plain),
                        CodeTokenKind::Plain,
                    ));
                }
                spans.push(CodeSpan::new(rest, CodeTokenKind::String));
                state.open_string = Some(delim);
                break;
            }
        }
        let ch = chars[i];
        if ch == '"' || ch == '\'' || ch == '`' {
            if ch == '`' && language_supports_multiline_backtick(language) {
                let rest: String = chars[i + 1..].iter().collect();
                if !rest.contains('`') {
                    if !plain.is_empty() {
                        spans.push(CodeSpan::new(
                            std::mem::take(&mut plain),
                            CodeTokenKind::Plain,
                        ));
                    }
                    let text: String = chars[i..].iter().collect();
                    spans.push(CodeSpan::new(text, CodeTokenKind::String));
                    state.open_string = Some(MultilineDelim::Backtick);
                    break;
                }
            }
            if !plain.is_empty() {
                spans.push(CodeSpan::new(
                    std::mem::take(&mut plain),
                    CodeTokenKind::Plain,
                ));
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
        if let Some(prefix) = line_comment(language) {
            let rest: String = chars[i..].iter().collect();
            if rest.starts_with(prefix) {
                if !plain.is_empty() {
                    spans.push(CodeSpan::new(
                        std::mem::take(&mut plain),
                        CodeTokenKind::Plain,
                    ));
                }
                spans.push(CodeSpan::new(rest, CodeTokenKind::Comment));
                break;
            }
        }
        if ch.is_ascii_digit() {
            if !plain.is_empty() {
                spans.push(CodeSpan::new(
                    std::mem::take(&mut plain),
                    CodeTokenKind::Plain,
                ));
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
                spans.push(CodeSpan::new(
                    std::mem::take(&mut plain),
                    CodeTokenKind::Plain,
                ));
            }
            if is_keyword(language, &word) {
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
        spans.push(CodeSpan::new(
            std::mem::take(&mut plain),
            CodeTokenKind::Plain,
        ));
    }
    if spans.is_empty() {
        spans.push(CodeSpan::new(line, CodeTokenKind::Plain));
    }
    spans
}

/// True when the slice opens a multiline string for `language`.
fn starts_multiline_open(language: &str, chars: &[char]) -> bool {
    match_multiline_open(chars).is_some_and(|delim| match delim {
        MultilineDelim::TripleDouble | MultilineDelim::TripleSingle => {
            language == "python" || language == "bash" || language == "ruby"
        }
        MultilineDelim::LuaBracket => language == "lua" || language == "sql",
        MultilineDelim::Backtick => false,
    })
}

/// Match a multiline string opener at the slice start, if any.
fn match_multiline_open(chars: &[char]) -> Option<MultilineDelim> {
    let rest: String = chars.iter().collect();
    if rest.starts_with("\"\"\"") {
        Some(MultilineDelim::TripleDouble)
    } else if rest.starts_with("'''") {
        Some(MultilineDelim::TripleSingle)
    } else if rest.starts_with("[[") {
        Some(MultilineDelim::LuaBracket)
    } else {
        None
    }
}

/// Whether an unclosed backtick continues on the next line.
fn language_supports_multiline_backtick(language: &str) -> bool {
    matches!(language, "javascript" | "typescript" | "go" | "rust")
}

/// Tokenize a whole fenced block threading one block state across lines.
/// Returns per-line spans in order; the caller keeps row alignment.
pub fn highlight_block(language: Option<&str>, lines: &[String]) -> Vec<Vec<CodeSpan>> {
    let mut state = HighlightBlockState::new();
    lines
        .iter()
        .map(|line| highlight_line_with_state(language, line, &mut state))
        .collect()
}

/// Memory profile of the highlight cache for budget asserts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HighlightMemoryProfile {
    /// Cache hits since creation.
    pub hits: u64,
    /// Cache misses since creation.
    pub misses: u64,
    /// Cached line entries.
    pub entries: usize,
    /// Source lines retained (equals entries for the per-line cache).
    pub lines: usize,
    /// Total spans across entries.
    pub spans: usize,
    /// Total source text bytes across entries.
    pub text_bytes: usize,
    /// Estimated cached bytes (`entries * estimate`).
    pub estimated_bytes: usize,
}

#[derive(Debug, Clone)]
struct CachedLine {
    spans: Vec<CodeSpan>,
    text_len: usize,
}

/// Bounded cache of highlighted source lines keyed by language plus content
/// hash. The key is a fixed 8-byte hash, so oversize lines never inflate the
/// key allocation. Block state participates in the key: identical text under
/// different block continuations caches separately.
#[derive(Debug, Clone, Default)]
pub struct HighlightCache {
    entries: HashMap<u64, CachedLine>,
    order: VecDeque<u64>,
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
        let mut state = HighlightBlockState::new();
        self.highlight_with_state(language, line, &mut state)
    }

    /// Highlight one line under an explicit block state, threading the
    /// continuation to the caller. The incoming state mixes into the key.
    pub fn highlight_with_state(
        &mut self,
        language: Option<&str>,
        line: &str,
        state: &mut HighlightBlockState,
    ) -> Vec<CodeSpan> {
        let incoming = *state;
        let key = cache_key(language, line, incoming);
        if let Some(cached) = self.entries.get(&key) {
            self.hits += 1;
            let spans = cached.spans.clone();
            let mut probe = incoming;
            let _ = highlight_line_with_state(language, line, &mut probe);
            *state = probe;
            return spans;
        }
        self.misses += 1;
        let mut next = incoming;
        let spans = highlight_line_with_state(language, line, &mut next);
        *state = next;
        let text_len = line.len();
        if self.entries.len() >= HIGHLIGHT_CACHE_CAPACITY && !self.entries.contains_key(&key) {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            }
        }
        self.order.push_back(key);
        self.entries.insert(
            key,
            CachedLine {
                spans: spans.clone(),
                text_len,
            },
        );
        spans
    }

    /// Highlight a whole block threading one state across lines through the
    /// shared cache. Row alignment is preserved: one output per input line.
    pub fn highlight_block(
        &mut self,
        language: Option<&str>,
        lines: &[String],
    ) -> Vec<Vec<CodeSpan>> {
        let mut state = HighlightBlockState::new();
        lines
            .iter()
            .map(|line| self.highlight_with_state(language, line, &mut state))
            .collect()
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
        self.entries
            .len()
            .saturating_mul(BYTES_PER_HIGHLIGHT_LINE_ESTIMATE)
    }

    /// Total source text bytes retained.
    pub fn text_bytes(&self) -> usize {
        self.entries.values().map(|entry| entry.text_len).sum()
    }

    /// Total spans retained.
    pub fn span_count(&self) -> usize {
        self.entries.values().map(|entry| entry.spans.len()).sum()
    }

    /// Full memory profile for budget asserts.
    pub fn memory_profile(&self) -> HighlightMemoryProfile {
        HighlightMemoryProfile {
            hits: self.hits,
            misses: self.misses,
            entries: self.entries.len(),
            lines: self.entries.len(),
            spans: self.span_count(),
            text_bytes: self.text_bytes(),
            estimated_bytes: self.bytes(),
        }
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

/// Cache key combining backend, normalized language, incoming block state
/// and the content hash. Fixed width: oversize lines hash to 8 bytes.
fn cache_key(language: Option<&str>, line: &str, state: HighlightBlockState) -> u64 {
    let normalized = normalize_language(language);
    let mut hash: u64 = 0xcbf29ce484222325;
    let mut mix = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    };
    mix(&HIGHLIGHT_BACKEND.to_le_bytes());
    mix(normalized.as_bytes());
    mix(&[0x1f, state.state_tag()]);
    mix(&content_hash(line).to_le_bytes());
    hash
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
        assert!(spans.iter().any(|span| span.kind == CodeTokenKind::Number));
        assert!(spans.iter().any(|span| span.kind == CodeTokenKind::Comment));
    }

    #[test]
    fn aliases_normalize_to_canonical_languages() {
        assert_eq!(normalize_language(Some("rs")), "rust");
        assert_eq!(normalize_language(Some("  TS ")), "typescript");
        assert_eq!(normalize_language(None), "");
        assert_eq!(normalize_language(Some("golang")), "go");
        assert_eq!(normalize_language(Some("cs")), "csharp");
        assert_eq!(normalize_language(Some("rb")), "ruby");
        assert_eq!(normalize_language(Some("shell")), "bash");
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

    #[test]
    fn block_comments_span_lines_with_state() {
        let mut state = HighlightBlockState::new();
        let first = highlight_line_with_state(Some("rust"), "/* open comment", &mut state);
        assert!(first.iter().any(|span| span.kind == CodeTokenKind::Comment));
        assert!(state.in_block_comment);
        let second = highlight_line_with_state(Some("rust"), "still comment */ done", &mut state);
        assert!(second
            .iter()
            .any(|span| span.kind == CodeTokenKind::Comment));
        assert!(!state.in_block_comment);
    }

    #[test]
    fn single_line_block_comment_stays_on_one_line() {
        let spans = highlight_line(Some("rust"), "let x = /* hidden */ 1;");
        assert!(spans.iter().any(|span| span.kind == CodeTokenKind::Comment));
        assert!(spans.iter().any(|span| span.kind == CodeTokenKind::Number));
    }

    #[test]
    fn python_triple_string_spans_lines() {
        let mut state = HighlightBlockState::new();
        let first = highlight_line_with_state(Some("python"), "\"\"\" doc start", &mut state);
        assert!(first.iter().any(|span| span.kind == CodeTokenKind::String));
        let second = highlight_line_with_state(Some("python"), "continued", &mut state);
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].kind, CodeTokenKind::String);
        let third = highlight_line_with_state(Some("python"), "end \"\"\" done", &mut state);
        assert!(third.iter().any(|span| span.kind == CodeTokenKind::String));
        assert!(state.open_string.is_none());
    }

    #[test]
    fn block_api_threads_state_for_whole_fence() {
        let lines = vec![
            "/* open".to_string(),
            "middle".to_string(),
            "close */ done".to_string(),
        ];
        let spans = highlight_block(Some("rust"), &lines);
        assert_eq!(spans.len(), 3);
        assert!(spans[0]
            .iter()
            .any(|span| span.kind == CodeTokenKind::Comment));
        assert!(spans[1]
            .iter()
            .any(|span| span.kind == CodeTokenKind::Comment));
        assert!(spans[2]
            .iter()
            .any(|span| span.kind == CodeTokenKind::Comment));
    }

    #[test]
    fn cache_key_is_fixed_width_for_oversize_lines() {
        let big = "x".repeat(64 * 1024);
        let key = cache_key(Some("rust"), &big, HighlightBlockState::new());
        let _ = key;
        let mut cache = HighlightCache::new();
        let first = cache.highlight(Some("rust"), &big);
        let second = cache.highlight(Some("rust"), &big);
        assert_eq!(first, second);
        assert_eq!(cache.hits(), 1);
        let profile = cache.memory_profile();
        assert_eq!(profile.entries, 1);
        assert_eq!(profile.lines, 1);
        assert!(profile.text_bytes >= big.len());
        assert_eq!(profile.estimated_bytes, cache.bytes());
    }

    #[test]
    fn memory_profile_reports_spans_and_text() {
        let mut cache = HighlightCache::new();
        cache.highlight(Some("rust"), "let x = 1;");
        cache.highlight(Some("python"), "x = 42");
        let profile = cache.memory_profile();
        assert_eq!(profile.entries, 2);
        assert_eq!(profile.lines, 2);
        assert!(profile.spans >= 2);
        assert!(profile.text_bytes > 0);
        assert_eq!(profile.hits, 0);
        assert_eq!(profile.misses, 2);
    }

    #[test]
    fn state_participates_in_cache_key() {
        let mut cache = HighlightCache::new();
        let mut inside = HighlightBlockState {
            in_block_comment: true,
            open_string: None,
        };
        let mut outside = HighlightBlockState::new();
        let a = cache.highlight_with_state(Some("rust"), "code", &mut inside);
        let b = cache.highlight_with_state(Some("rust"), "code", &mut outside);
        assert_ne!(a, b);
    }

    #[test]
    fn go_and_java_keywords_classify() {
        let spans = highlight_line(Some("go"), "func main() { return }");
        assert!(spans
            .iter()
            .any(|span| span.text == "func" && span.kind == CodeTokenKind::Keyword));
        let spans = highlight_line(Some("java"), "class Main { return; }");
        assert!(spans
            .iter()
            .any(|span| span.text == "class" && span.kind == CodeTokenKind::Keyword));
    }
}
