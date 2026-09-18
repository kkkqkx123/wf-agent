//! Shared reasoning-segment contract for streaming text.
//!
//! The sentinel and escape set are the single source of truth for both the
//! arrival-order pacer and the markdown renderer, so reasoning chunks keep
//! arrival order and parse stably across frames.

/// Zero-width marker wrapping a reasoning segment.
pub const REASONING_SENTINEL: char = '\u{2063}';

/// Emphasis characters escaped inside reasoning text.
pub const REASONING_ESCAPES: &[char] = &['*', '_', '~'];

/// Wrap `text` as one reasoning segment for the shared ordered stream.
pub fn mark_reasoning(text: &str) -> String {
    format!("{REASONING_SENTINEL}{text}{REASONING_SENTINEL}")
}

/// Split `marked` on the reasoning sentinel into ordered segments.
pub fn split_reasoning_marks(marked: &str) -> Vec<(bool, String)> {
    marked
        .split(REASONING_SENTINEL)
        .enumerate()
        .filter(|(_, piece)| !piece.is_empty())
        .map(|(index, piece)| (index % 2 == 1, piece.to_string()))
        .collect()
}

/// Escape emphasis characters so markers stay adjacent to non-space content.
pub fn escape_reasoning(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if REASONING_ESCAPES.contains(&ch) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// Recover the original markdown from a marked reasoning segment.
pub fn reasoning_line_content(marked: &str) -> String {
    marked.replace(REASONING_SENTINEL, "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_round_trip_in_arrival_order() {
        let marked = format!("answer {s}thinking{s} tail", s = REASONING_SENTINEL);
        assert_eq!(
            split_reasoning_marks(&marked),
            vec![
                (false, "answer ".to_string()),
                (true, "thinking".to_string()),
                (false, " tail".to_string()),
            ]
        );
    }

    #[test]
    fn escapes_keep_emphasis_adjacent() {
        assert_eq!(escape_reasoning("a*b_c~d"), "a\\*b\\_c\\~d");
    }

    #[test]
    fn content_recovery_strips_sentinels() {
        let marked = mark_reasoning("thinking");
        assert_eq!(reasoning_line_content(&marked), "thinking");
    }
}
