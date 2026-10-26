//! Syntax highlighting core backed by tree-sitter.
//!
//! The crate exposes a neutral span model and a bounded cache. UI crates map
//! the spans to their own style systems; this crate has no rendering
//! dependency.

use std::collections::{HashMap, VecDeque};
use std::sync::OnceLock;

use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};

pub const HIGHLIGHT_VERSION: u64 = 3;
pub const HIGHLIGHT_BACKEND: u64 = 2;
pub const HIGHLIGHT_BACKEND_NAME: &str = "tree-sitter";

pub const HIGHLIGHT_CACHE_CAPACITY: usize = 256;
pub const BYTES_PER_HIGHLIGHT_LINE_ESTIMATE: usize = 512;

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

/// Fenced-code alias table shared with the web frontend. The TSV file is the
/// single source of truth; the frontend generates a TS module from it at
/// build time and both sides carry a consistency test against this file.
static LANGUAGE_ALIASES: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();

fn language_aliases() -> &'static HashMap<&'static str, &'static str> {
    LANGUAGE_ALIASES.get_or_init(|| {
        include_str!("language-aliases.tsv")
            .lines()
            .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
            .filter_map(|line| line.split_once('\t'))
            .map(|(alias, canonical)| (alias.trim(), canonical.trim()))
            .collect()
    })
}

pub fn normalize_language(language: Option<&str>) -> String {
    let raw = language.unwrap_or("").trim().to_ascii_lowercase();
    language_aliases()
        .get(raw.as_str())
        .copied()
        .unwrap_or(&raw)
        .to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HighlightBlockState;

impl HighlightBlockState {
    pub fn new() -> Self {
        Self
    }
}

struct LanguageSpec {
    language: tree_sitter::Language,
    query: Query,
}

fn build_specs() -> HashMap<&'static str, LanguageSpec> {
    let mut specs = HashMap::new();
    macro_rules! add {
        ($key:expr, $lang:expr, $query:expr) => {
            if let Ok(query) = Query::new(&$lang, $query) {
                specs.insert(
                    $key,
                    LanguageSpec {
                        language: $lang,
                        query,
                    },
                );
            }
        };
    }

    add!(
        "bash",
        tree_sitter_bash::LANGUAGE.into(),
        include_str!("../queries/bash/highlights.scm")
    );
    add!(
        "c",
        tree_sitter_c::LANGUAGE.into(),
        include_str!("../queries/c/highlights.scm")
    );
    add!(
        "cpp",
        tree_sitter_cpp::LANGUAGE.into(),
        include_str!("../queries/cpp/highlights.scm")
    );
    add!(
        "csharp",
        tree_sitter_c_sharp::LANGUAGE.into(),
        include_str!("../queries/csharp/highlights.scm")
    );
    add!(
        "css",
        tree_sitter_css::LANGUAGE.into(),
        include_str!("../queries/css/highlights.scm")
    );
    add!(
        "dart",
        tree_sitter_dart::LANGUAGE.into(),
        include_str!("../queries/dart/highlights.scm")
    );
    add!(
        "go",
        tree_sitter_go::LANGUAGE.into(),
        include_str!("../queries/go/highlights.scm")
    );
    add!(
        "html",
        tree_sitter_html::LANGUAGE.into(),
        include_str!("../queries/html/highlights.scm")
    );
    add!(
        "java",
        tree_sitter_java::LANGUAGE.into(),
        include_str!("../queries/java/highlights.scm")
    );
    add!(
        "javascript",
        tree_sitter_javascript::LANGUAGE.into(),
        include_str!("../queries/javascript/highlights.scm")
    );
    add!(
        "lua",
        tree_sitter_lua::LANGUAGE.into(),
        include_str!("../queries/lua/highlights.scm")
    );
    add!(
        "php",
        tree_sitter_php::LANGUAGE_PHP.into(),
        include_str!("../queries/php/highlights.scm")
    );
    add!(
        "python",
        tree_sitter_python::LANGUAGE.into(),
        include_str!("../queries/python/highlights.scm")
    );
    add!(
        "ruby",
        tree_sitter_ruby::LANGUAGE.into(),
        include_str!("../queries/ruby/highlights.scm")
    );
    add!(
        "rust",
        tree_sitter_rust::LANGUAGE.into(),
        include_str!("../queries/rust/highlights.scm")
    );
    add!(
        "scala",
        tree_sitter_scala::LANGUAGE.into(),
        include_str!("../queries/scala/highlights.scm")
    );
    add!(
        "typescript",
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        concat!(
            include_str!("../queries/javascript/highlights.scm"),
            "\n",
            include_str!("../queries/typescript/highlights.scm")
        )
    );

    specs
}

static SPECS: OnceLock<HashMap<&'static str, LanguageSpec>> = OnceLock::new();

fn specs() -> &'static HashMap<&'static str, LanguageSpec> {
    SPECS.get_or_init(build_specs)
}

fn spec(normalized: &str) -> Option<&'static LanguageSpec> {
    specs().get(normalized)
}

fn capture_kind(name: &str) -> CodeTokenKind {
    if name.contains("language") || name.contains("boolean") {
        return CodeTokenKind::Keyword;
    }
    let base = name.split('.').next().unwrap_or(name);
    match base {
        "comment" | "documentation" => CodeTokenKind::Comment,
        "string" | "char" | "regex" => CodeTokenKind::String,
        "number" | "float" | "integer" | "constant" => CodeTokenKind::Number,
        "keyword" | "conditional" | "repeat" | "label" | "exception" => CodeTokenKind::Keyword,
        "function" | "method" | "constructor" | "macro" => CodeTokenKind::Function,
        "type" | "class" | "struct" | "enum" | "interface" | "trait" | "namespace" | "module" => {
            CodeTokenKind::Type
        }
        _ => CodeTokenKind::Plain,
    }
}

pub fn highlight_block(language: Option<&str>, lines: &[String]) -> Vec<Vec<CodeSpan>> {
    let normalized = normalize_language(language);
    let Some(spec) = spec(&normalized) else {
        return lines
            .iter()
            .map(|line| vec![CodeSpan::new(line.clone(), CodeTokenKind::Plain)])
            .collect();
    };

    let source = lines.join("\n");
    let mut parser = Parser::new();
    if parser.set_language(&spec.language).is_err() {
        return lines
            .iter()
            .map(|line| vec![CodeSpan::new(line.clone(), CodeTokenKind::Plain)])
            .collect();
    }
    let Some(tree) = parser.parse(&source, None) else {
        return lines
            .iter()
            .map(|line| vec![CodeSpan::new(line.clone(), CodeTokenKind::Plain)])
            .collect();
    };

    let mut cursor = QueryCursor::new();
    let mut ranges: Vec<(usize, usize, &'static str)> = Vec::new();
    let mut matches = cursor.matches(&spec.query, tree.root_node(), source.as_bytes());
    let capture_names = spec.query.capture_names();
    while let Some(m) = matches.next() {
        for capture in m.captures {
            let name = capture_names[capture.index as usize];
            ranges.push((capture.node.start_byte(), capture.node.end_byte(), name));
        }
    }
    ranges.sort_by(|a, b| a.0.cmp(&b.0).then((b.1 - b.0).cmp(&(a.1 - a.0))));

    let mut line_starts = Vec::with_capacity(lines.len());
    let mut acc = 0usize;
    for line in lines {
        line_starts.push(acc);
        acc += line.len() + 1;
    }

    let mut out = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        if line.is_empty() {
            out.push(vec![CodeSpan::new(String::new(), CodeTokenKind::Plain)]);
            continue;
        }
        let line_start = line_starts[i];
        let line_end = line_start + line.len();
        let mut color_map = vec![None; line.len()];
        for (start, end, name) in &ranges {
            let s = (*start).max(line_start);
            let e = (*end).min(line_end);
            if s >= e {
                continue;
            }
            let rel_s = s - line_start;
            let rel_e = e - line_start;
            for slot in color_map.iter_mut().take(rel_e).skip(rel_s) {
                *slot = Some(*name);
            }
        }

        let mut spans = Vec::new();
        let mut current: Option<&str> = None;
        let mut buf = String::new();
        for (byte_index, ch) in line.char_indices() {
            let color = color_map[byte_index];
            if color != current && !buf.is_empty() {
                spans.push(CodeSpan::new(
                    std::mem::take(&mut buf),
                    current.map(capture_kind).unwrap_or(CodeTokenKind::Plain),
                ));
            }
            current = color;
            buf.push(ch);
        }
        if !buf.is_empty() {
            spans.push(CodeSpan::new(
                buf,
                current.map(capture_kind).unwrap_or(CodeTokenKind::Plain),
            ));
        }
        if spans.is_empty() {
            spans.push(CodeSpan::new(line.clone(), CodeTokenKind::Plain));
        }
        out.push(spans);
    }

    out
}

pub fn highlight_line(language: Option<&str>, line: &str) -> Vec<CodeSpan> {
    highlight_block(language, &[line.to_string()])
        .into_iter()
        .next()
        .unwrap_or_else(|| vec![CodeSpan::new(line, CodeTokenKind::Plain)])
}

pub fn highlight_line_with_state(
    language: Option<&str>,
    line: &str,
    _state: &mut HighlightBlockState,
) -> Vec<CodeSpan> {
    highlight_line(language, line)
}

#[derive(Debug, Clone, Default)]
pub struct HighlightCache {
    entries: HashMap<u64, Vec<CodeSpan>>,
    order: VecDeque<u64>,
    hits: u64,
    misses: u64,
}

impl HighlightCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn highlight(&mut self, language: Option<&str>, line: &str) -> Vec<CodeSpan> {
        let mut state = HighlightBlockState::new();
        self.highlight_with_state(language, line, &mut state)
    }

    pub fn highlight_with_state(
        &mut self,
        language: Option<&str>,
        line: &str,
        _state: &mut HighlightBlockState,
    ) -> Vec<CodeSpan> {
        let key = fnv1a(format!("{:?}:{}", language, line).as_bytes());
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
        self.order.push_back(key);
        self.entries.insert(key, spans.clone());
        spans
    }

    pub fn highlight_block(
        &mut self,
        language: Option<&str>,
        lines: &[String],
    ) -> Vec<Vec<CodeSpan>> {
        highlight_block(language, lines)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn bytes(&self) -> usize {
        self.entries
            .len()
            .saturating_mul(BYTES_PER_HIGHLIGHT_LINE_ESTIMATE)
    }
    pub fn hits(&self) -> u64 {
        self.hits
    }
    pub fn misses(&self) -> u64 {
        self.misses
    }
    pub fn memory_profile(&self) -> HighlightMemoryProfile {
        HighlightMemoryProfile {
            hits: self.hits,
            misses: self.misses,
            entries: self.entries.len(),
            lines: self.entries.len(),
            spans: self.entries.values().map(|v| v.len()).sum(),
            text_bytes: 0,
            estimated_bytes: self.bytes(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HighlightMemoryProfile {
    pub hits: u64,
    pub misses: u64,
    pub entries: usize,
    pub lines: usize,
    pub spans: usize,
    pub text_bytes: usize,
    pub estimated_bytes: usize,
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

pub fn content_hash(line: &str) -> u64 {
    fnv1a(line.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typescript_combined_query_is_valid() {
        let language: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        let source = concat!(
            include_str!("../queries/javascript/highlights.scm"),
            "\n",
            include_str!("../queries/typescript/highlights.scm")
        );
        assert!(
            Query::new(&language, source).is_ok(),
            "combined TypeScript query must parse"
        );
    }

    #[test]
    fn all_language_specs_load() {
        let expected = [
            "bash",
            "c",
            "cpp",
            "csharp",
            "css",
            "dart",
            "go",
            "html",
            "java",
            "javascript",
            "lua",
            "php",
            "python",
            "ruby",
            "rust",
            "scala",
            "typescript",
        ];
        for language in expected {
            assert!(
                specs().contains_key(language),
                "missing or invalid highlight spec for {language}"
            );
        }
    }

    #[test]
    fn all_language_specs_highlight_trivial_input() {
        for language in specs().keys() {
            let lines = highlight_block(Some(language), &["x".to_string()]);
            assert_eq!(
                lines.len(),
                1,
                "highlight_block returned wrong line count for {language}"
            );
            assert!(
                !lines[0].is_empty(),
                "highlight_block returned no spans for {language}"
            );
        }
    }

    #[test]
    fn alias_table_is_wellformed_and_applied() {
        let raw = include_str!("language-aliases.tsv");
        let mut count = 0;
        for line in raw.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (alias, canonical) = line
                .split_once('\t')
                .unwrap_or_else(|| panic!("alias line missing tab separator: {line}"));
            assert!(
                !alias.is_empty() && !canonical.is_empty(),
                "blank field in: {line}"
            );
            assert_eq!(
                normalize_language(Some(alias)),
                canonical,
                "alias not applied"
            );
            count += 1;
        }
        assert!(count > 0, "alias table must not be empty");
    }
}
