//! Canonical markdown parser dialect for every wf-agent renderer.
//!
//! Both consumers (`tui-markdown` and `wf-mini`) must build their parsers
//! with [`parser_options`] so the same assistant text renders identically in
//! the line-discipline stdout and the fullscreen TUI. Changing the dialect
//! here changes every renderer at once: evaluate both consumers before
//! adding or removing an option, and keep the member-locking unit tests in
//! sync with the decision recorded in `docs/plan/markdown-parser-extensions.md`.
//!
//! Current dialect: CommonMark plus tables, task lists and strikethrough.
//! Deliberately excluded: smart punctuation (rewrites literal text),
//! metadata blocks (front matter would leak into rendered output), math,
//! wikilinks, GFM blockquote tags, footnotes and definition lists (no
//! rendering branches designed yet).

use pulldown_cmark::Options;

/// Canonical parser options shared by every renderer.
pub fn parser_options() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH
}

/// True when the source tail is table-risky: the last non-blank line
/// contains a `|`, so streaming renderers must hold back output until the
/// construct settles (a later line without `|`, a blank line, or a flush
/// boundary). Conservative by design — table rows, delimiters and plain
/// text with an inline `|` all hold; nothing that was already emitted is
/// ever rewritten on the append-only stdout.
pub fn has_table_risk(src: &str) -> bool {
    src.lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .is_some_and(|line| line.contains('|'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialect_enables_exactly_three_extensions() {
        let options = parser_options();
        assert!(options.contains(Options::ENABLE_TABLES));
        assert!(options.contains(Options::ENABLE_TASKLISTS));
        assert!(options.contains(Options::ENABLE_STRIKETHROUGH));
        assert_eq!(
            options,
            Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH
        );
        assert!(!options.contains(Options::ENABLE_SMART_PUNCTUATION));
        assert!(!options.contains(Options::ENABLE_FOOTNOTES));
        assert!(!options.contains(Options::ENABLE_YAML_STYLE_METADATA_BLOCKS));
    }

    #[test]
    fn table_risk_tracks_last_non_blank_line() {
        assert!(has_table_risk("| a | b |"));
        assert!(has_table_risk("| a |\n| --- |\n"));
        assert!(has_table_risk("text with `code | pipe`\n"));
        assert!(!has_table_risk("plain paragraph\n"));
        assert!(!has_table_risk("| a |\n\nsettled after blank line\n"));
        assert!(!has_table_risk(""));
    }
}
