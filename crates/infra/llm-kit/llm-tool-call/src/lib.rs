//! Tool-call protocol parsing for the llm-kit group.
//!
//! Protocol parsers are gated behind feature flags so callers only pay for
//! what they use:
//! - `json-partial`: streaming partial-JSON argument recovery;
//! - `xml`: XML text-protocol tool-call parsing and prompt rendering.
//!
//! Source files were upstreamed from `wf-llm`; `wf-llm` consumes this crate
//! (re-export) so the tool-call protocol has a single canonical implementation.

#[cfg(feature = "json-partial")]
pub mod partial_json_parser;

#[cfg(feature = "xml")]
pub mod tool;

#[cfg(test)]
mod tests {
    // Compile-time smoke checks: each feature module exposes its entry point.
    #[cfg(feature = "json-partial")]
    #[test]
    fn partial_json_entry_point_exists() {
        let result = crate::partial_json_parser::parse_partial_json("{\"a\": 1}");
        assert!(result.as_complete().is_some());
    }

    #[cfg(feature = "xml")]
    #[test]
    fn xml_entry_point_exists() {
        let options = crate::tool::parser::ToolCallParseOptions::default();
        let parsed = crate::tool::parser::parse_partial(
            "<tool_use><tool_name>t1</tool_name><parameters><q>1</q></parameters></tool_use>",
            &options,
        );
        assert_eq!(parsed.len(), 1);
    }
}
