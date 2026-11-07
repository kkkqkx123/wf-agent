//! Shared segmentation core for keyword indexes.
//!
//! Pure text handling with no engine types so embedded, single-file and
//! remote backends share one word outcome. English keeps the legacy
//! lowercase alphanumeric splitting; mixed adds dictionary segmentation
//! for Chinese plus identifier splitting for code-like words.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use wf_types::config::keyword::KeywordTokenizer;

/// Upper bound on analyzed characters; longer inputs truncate the prefix.
pub const MAX_TOKENIZE_CHARS: usize = 32_768;
/// Character window for dictionary segmentation of long CJK runs.
pub const MAX_CJK_RUN_CHARS: usize = 4096;

/// One analyzed token with byte offsets and BM25 position data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MixedToken {
    /// Lowercased search form.
    pub text: String,
    /// Byte start in the truncated input.
    pub offset_from: usize,
    /// Byte end in the truncated input.
    pub offset_to: usize,
    /// Word position; original and subwords share one position.
    pub position: u32,
    /// Original spans one slot, subwords span none.
    pub position_length: u32,
}

/// Host-contributed analyzer replacing the built-in mixed segmentation.
/// Pure mapping from text to search tokens; network access is forbidden.
pub type CustomAnalyzer = Arc<dyn Fn(String) -> Vec<String> + Send + Sync>;

fn registry() -> &'static Mutex<HashMap<String, CustomAnalyzer>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, CustomAnalyzer>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Registers a host analyzer under `name`, replacing any prior entry.
pub fn register_analyzer(name: &str, analyzer: CustomAnalyzer) {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return;
    }
    if let Ok(mut guard) = registry().lock() {
        guard.insert(trimmed.to_string(), analyzer);
    }
}

/// Finds a registered host analyzer by name.
pub fn find_analyzer(name: &str) -> Option<CustomAnalyzer> {
    registry()
        .lock()
        .ok()
        .and_then(|guard| guard.get(name.trim()).cloned())
}

/// Clears registered host analyzers; tests use it to isolate cases.
pub fn clear_analyzers() {
    if let Ok(mut guard) = registry().lock() {
        guard.clear();
    }
}

/// Fails when the named analyzer is requested but not registered.
pub fn ensure_analyzer_registered(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("analyzer name must not be empty".to_string());
    }
    match find_analyzer(name) {
        Some(_) => Ok(()),
        None => Err(format!("unknown text analyzer '{name}'")),
    }
}

fn shared_jieba() -> &'static jieba_rs::Jieba {
    static JIEBA: OnceLock<jieba_rs::Jieba> = OnceLock::new();
    JIEBA.get_or_init(jieba_rs::Jieba::new)
}

/// Whether a character belongs to a CJK script run.
pub fn is_cjk(ch: char) -> bool {
    matches!(ch,
        '\u{3400}'..='\u{4DBF}'
        | '\u{4E00}'..='\u{9FFF}'
        | '\u{20000}'..='\u{2A6DF}'
        | '\u{3040}'..='\u{309F}'
        | '\u{30A0}'..='\u{30FF}'
        | '\u{AC00}'..='\u{D7AF}'
        | '\u{1100}'..='\u{11FF}')
}

/// Splits an identifier into lowercase component words.
pub fn split_identifier(ident: &str) -> Vec<String> {
    if ident.is_empty() {
        return Vec::new();
    }
    let mut result = Vec::new();
    for segment in ident.split(['_', '-', '.', '/', '\\']) {
        if segment.is_empty() {
            continue;
        }
        for sub in segment.split("::") {
            if sub.is_empty() {
                continue;
            }
            for sub2 in sub.split(':') {
                if sub2.is_empty() {
                    continue;
                }
                result.extend(split_camel_case_words(sub2));
            }
        }
    }
    result
        .into_iter()
        .filter(|word| word.len() >= 2 && !word.chars().all(|ch| ch.is_ascii_digit()))
        .collect()
}

/// Splits one camel-case word into lowercase parts.
pub fn split_camel_case_words(word: &str) -> Vec<String> {
    if word.is_empty() {
        return Vec::new();
    }
    let mut result = Vec::new();
    for segment in word.split(['_', '-']) {
        if segment.is_empty() {
            continue;
        }
        let chars: Vec<char> = segment.chars().collect();
        let mut current = String::new();
        for i in 0..chars.len() {
            let ch = chars[i];
            if ch.is_uppercase() && !current.is_empty() {
                let prev_is_lower = i > 0 && chars[i - 1].is_lowercase();
                let next_is_lower = i + 1 < chars.len() && chars[i + 1].is_lowercase();
                if prev_is_lower || next_is_lower {
                    result.push(current.to_lowercase());
                    current.clear();
                }
            } else if !current.is_empty() {
                let last = current.chars().last().unwrap_or(' ');
                if (ch.is_ascii_digit() && !last.is_ascii_digit())
                    || (ch.is_alphabetic() && last.is_ascii_digit())
                {
                    result.push(current.to_lowercase());
                    current.clear();
                }
            }
            current.push(ch);
        }
        if !current.is_empty() {
            result.push(current.to_lowercase());
        }
    }
    result
}

/// Legacy English terms: lowercase alphanumeric splitting.
pub fn english_terms(text: &str) -> Vec<String> {
    text.split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_lowercase())
        .collect()
}

/// Built-in mixed terms without host analyzers.
pub fn mixed_terms(text: &str) -> Vec<String> {
    analyze_mixed(text)
        .into_iter()
        .map(|token| token.text)
        .collect()
}

/// Terms for one tokenizer with an optional host analyzer override.
/// Unknown analyzer names fall back to the built-in path here; backends
/// validate with [`ensure_analyzer_registered`] so use fails fast.
pub fn terms(text: &str, tokenizer: KeywordTokenizer, analyzer_name: Option<&str>) -> Vec<String> {
    if let Some(name) = analyzer_name.filter(|name| !name.trim().is_empty()) {
        if let Some(analyzer) = find_analyzer(name) {
            return analyzer(text.to_string())
                .into_iter()
                .map(|term| term.trim().to_string())
                .filter(|term| !term.is_empty())
                .collect();
        }
    }
    match tokenizer {
        KeywordTokenizer::English => english_terms(text),
        KeywordTokenizer::Mixed => mixed_terms(text),
    }
}

/// Space-joined pretokenized form for whitespace-analyzed sidecars.
pub fn pretokenize(text: &str, tokenizer: KeywordTokenizer, analyzer_name: Option<&str>) -> String {
    terms(text, tokenizer, analyzer_name).join(" ")
}

/// Position count of analyzed tokens for budget accounting.
pub fn position_count(tokens: &[MixedToken]) -> usize {
    tokens
        .iter()
        .map(|token| token.position as usize + 1)
        .max()
        .unwrap_or(0)
}

/// Analyzes text with position and offset metadata.
pub fn analyze_mixed(text: &str) -> Vec<MixedToken> {
    let truncated: String = text.chars().take(MAX_TOKENIZE_CHARS).collect();
    if truncated.trim().is_empty() {
        return Vec::new();
    }
    let chars: Vec<(usize, char)> = truncated.char_indices().collect();
    let mut out = Vec::new();
    let mut position: u32 = 0;
    let mut index = 0;
    while index < chars.len() {
        let cjk = is_cjk(chars[index].1);
        let mut end = index + 1;
        while end < chars.len() && is_cjk(chars[end].1) == cjk {
            end += 1;
        }
        let byte_from = chars[index].0;
        let byte_to = if end < chars.len() {
            chars[end].0
        } else {
            truncated.len()
        };
        let slice = &truncated[byte_from..byte_to];
        if cjk {
            position = push_cjk_slice(slice, byte_from, position, &mut out);
        } else {
            position = push_latin_slice(slice, byte_from, position, &mut out);
        }
        index = end;
    }
    out
}

fn push_latin_slice(
    slice: &str,
    slice_base: usize,
    mut position: u32,
    out: &mut Vec<MixedToken>,
) -> u32 {
    let mut cursor = 0usize;
    let mut word_start: Option<usize> = None;
    for (offset, ch) in slice.char_indices() {
        if ch.is_whitespace() {
            if let Some(start) = word_start.take() {
                position = push_latin_word(slice, slice_base, start, cursor, position, out);
            }
            cursor = offset + ch.len_utf8();
        } else if word_start.is_none() {
            word_start = Some(offset);
            cursor = offset + ch.len_utf8();
        } else {
            cursor = offset + ch.len_utf8();
        }
    }
    if let Some(start) = word_start {
        position = push_latin_word(slice, slice_base, start, cursor, position, out);
    }
    position
}

fn push_latin_word(
    slice: &str,
    slice_base: usize,
    start: usize,
    end: usize,
    position: u32,
    out: &mut Vec<MixedToken>,
) -> u32 {
    let raw = &slice[start..end];
    let trimmed_start = raw
        .char_indices()
        .find(|(_, ch)| ch.is_alphanumeric())
        .map(|(offset, _)| offset)
        .unwrap_or(raw.len());
    let trimmed_end = raw
        .char_indices()
        .rev()
        .find(|(_, ch)| ch.is_alphanumeric())
        .map(|(offset, ch)| offset + ch.len_utf8())
        .unwrap_or(0);
    if trimmed_start >= trimmed_end {
        return position;
    }
    let trimmed = &raw[trimmed_start..trimmed_end];
    let lowered = trimmed.to_lowercase();
    if lowered.is_empty() {
        return position;
    }
    out.push(MixedToken {
        text: lowered.clone(),
        offset_from: slice_base + start + trimmed_start,
        offset_to: slice_base + start + trimmed_end,
        position,
        position_length: 1,
    });
    for part in split_identifier(trimmed) {
        if part == lowered {
            continue;
        }
        out.push(MixedToken {
            text: part,
            offset_from: slice_base + start + trimmed_start,
            offset_to: slice_base + start + trimmed_end,
            position,
            position_length: 0,
        });
    }
    position.saturating_add(1)
}

fn push_cjk_slice(
    slice: &str,
    slice_base: usize,
    mut position: u32,
    out: &mut Vec<MixedToken>,
) -> u32 {
    let chars: Vec<char> = slice.chars().collect();
    let mut window_start = 0;
    while window_start < chars.len() {
        let window_end = (window_start + MAX_CJK_RUN_CHARS).min(chars.len());
        let window: String = chars[window_start..window_end].iter().collect();
        let window_base = slice_base + window_start_bytes(slice, window_start);
        for token in shared_jieba().cut_for_search(&window, true) {
            if token.word.trim().is_empty() {
                continue;
            }
            out.push(MixedToken {
                text: token.word.to_lowercase(),
                offset_from: window_base + token.byte_start,
                offset_to: window_base + token.byte_end,
                position,
                position_length: 1,
            });
            position = position.saturating_add(1);
        }
        window_start = window_end;
    }
    position
}

fn window_start_bytes(slice: &str, window_start: usize) -> usize {
    slice
        .chars()
        .take(window_start)
        .map(|ch| ch.len_utf8())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_keeps_legacy_splitting() {
        assert_eq!(english_terms("Rust Ownership!"), vec!["rust", "ownership"]);
        assert_eq!(
            terms("Hello World", KeywordTokenizer::English, None),
            english_terms("Hello World")
        );
    }

    #[test]
    fn mixed_segments_chinese_and_splits_identifiers() {
        clear_analyzers();
        let tokens = mixed_terms("中文分词 getUserById");
        assert!(tokens
            .iter()
            .any(|term| term == "getuserbyid" || term == "get"));
        assert!(tokens.iter().any(|term| term == "get" || term == "user"));
        assert!(tokens.len() >= 3);
    }

    #[test]
    fn mixed_positions_share_identifier_slots() {
        let analyzed = analyze_mixed("getUserById");
        assert!(!analyzed.is_empty());
        let first_position = analyzed[0].position;
        assert!(analyzed
            .iter()
            .all(|token| token.position == first_position));
        assert_eq!(analyzed[0].position_length, 1);
        assert!(analyzed[1..].iter().all(|token| token.position_length == 0));
        assert_eq!(position_count(&analyzed), 1);
    }

    #[test]
    fn custom_analyzer_overrides_builtin() {
        clear_analyzers();
        register_analyzer(
            "test-custom",
            Arc::new(|text: String| vec![format!("custom-{text}")]),
        );
        let got = terms("hello", KeywordTokenizer::English, Some("test-custom"));
        assert_eq!(got, vec!["custom-hello"]);
        assert!(ensure_analyzer_registered("test-custom").is_ok());
        assert!(ensure_analyzer_registered("missing").is_err());
        clear_analyzers();
    }
}
