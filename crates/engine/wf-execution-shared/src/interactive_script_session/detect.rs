//! Prompt-pattern detection over a session output tail.

/// Match the tail of the session output against the configured prompt
/// patterns. Invalid patterns are ignored. Returns the matched pattern.
pub fn detect_prompt(tail: &str, patterns: &[String]) -> Option<String> {
    for pattern in patterns {
        let Ok(re) = regex::Regex::new(pattern) else {
            continue;
        };
        if re.is_match(tail) {
            return Some(pattern.clone());
        }
    }
    None
}
