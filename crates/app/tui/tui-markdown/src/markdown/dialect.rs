//! Canonical markdown parser dialect for every wf-agent renderer.
//!
//! Both consumers (`tui-markdown` itself and `wf-mini`) must build their
//! parsers with [`parser_options`] so the same assistant text renders
//! identically in the line-discipline stdout and the fullscreen TUI.
//! Changing the dialect here changes every renderer at once: evaluate both
//! consumers before adding or removing an option, and keep the
//! member-locking unit tests in sync with that decision.
//!
//! Current dialect: CommonMark plus tables, task lists, strikethrough,
//! math, footnotes and definition lists. Deliberately excluded: smart
//! punctuation (rewrites literal text), metadata blocks (front matter would
//! leak into rendered output), wikilinks and GFM blockquote tags (no
//! rendering branches designed yet).
//!
//! [`DIALECT_VERSION`] identifies this option set for preparation and render
//! cache keys: any option change must bump it so stale laid-out rows never
//! survive a dialect switch.

use pulldown_cmark::Options;

/// Version of the canonical option set, folded into cache keys.
pub const DIALECT_VERSION: u64 = 2;

/// Canonical parser options shared by every renderer.
pub fn parser_options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_MATH
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_DEFINITION_LIST
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

/// True when the source carries a reference-style link definition or use.
/// A later definition rewrites earlier link targets, so streaming renderers
/// must hold back the whole source until finalize instead of emitting prefix
/// output that a later line would retroactively change. Conservative by
/// design: any `][` substring counts as a potential reference use, and any
/// line starting with `[` and containing `]:` counts as a definition.
pub fn has_reference_definition(src: &str) -> bool {
    if src.contains("][") {
        return true;
    }
    for line in src.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('[') {
            if let Some(close) = trimmed.find(']') {
                if trimmed[close..].starts_with("]:") {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialect_enables_the_canonical_extensions() {
        let options = parser_options();
        assert!(options.contains(Options::ENABLE_TABLES));
        assert!(options.contains(Options::ENABLE_TASKLISTS));
        assert!(options.contains(Options::ENABLE_STRIKETHROUGH));
        assert!(options.contains(Options::ENABLE_MATH));
        assert!(options.contains(Options::ENABLE_FOOTNOTES));
        assert!(options.contains(Options::ENABLE_DEFINITION_LIST));
        assert_eq!(
            options,
            Options::ENABLE_TABLES
                | Options::ENABLE_TASKLISTS
                | Options::ENABLE_STRIKETHROUGH
                | Options::ENABLE_MATH
                | Options::ENABLE_FOOTNOTES
                | Options::ENABLE_DEFINITION_LIST
        );
        assert!(!options.contains(Options::ENABLE_SMART_PUNCTUATION));
        assert!(!options.contains(Options::ENABLE_YAML_STYLE_METADATA_BLOCKS));
        assert_eq!(DIALECT_VERSION, 2);
    }

    #[test]
    fn reference_definition_detection_is_conservative() {
        assert!(has_reference_definition(
            "click [here][l]\n\n[l]: https://x.test\n"
        ));
        assert!(has_reference_definition("see [a][b] now"));
        assert!(has_reference_definition("[l]: https://x.test\n"));
        assert!(!has_reference_definition("plain **bold** text\n"));
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
