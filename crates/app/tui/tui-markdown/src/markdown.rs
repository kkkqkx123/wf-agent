//! Markdown rendering for the TUI.
//!
//! This module groups six responsibilities, each in its own submodule:
//!
//! * [`stream`] — the incremental streaming state machine ([`MarkdownStream`],
//!   [`MarkdownFrame`], [`ParseMetrics`]) that splits a growing source into a
//!   settled (committed) region and an in-flight (streaming) region.
//! * [`blocks`] — stateless block-structure analysis (fences, tables,
//!   reference definitions, blank-line settlement) used to compute the split.
//! * [`plain`] — whole-source Markdown → plain text ([`render_plain_text`]).
//! * [`styled`] — whole-source Markdown → styled ratatui `Line`s
//!   ([`render_styled_lines`], [`render_styled_lines_animated`]).
//! * [`document`] — backend-neutral semantic model ([`crate::markdown::document::Document`],
//!   [`crate::markdown::document::StyleRole`], [`crate::markdown::document::StyledSpan`]) shared by every frontend.
//! * [`highlight`] — dependency-free per-language code tokenizer plus the
//!   bounded [`highlight::HighlightCache`] behind fenced code coloring.
//! * [`math`] — LaTeX to Unicode fallback ([`math::latex_to_unicode`]) for
//!   the readable text beside each raw `latex` expression, plus delimiter
//!   normalization and display-math render modes.
//! * [`render_options`] — explicit spacing, chart and latex switches
//!   threaded through parsing and layout and folded into cache keys.
//! * [`dialect`] — the canonical parser dialect (pulldown-cmark options and
//!   streaming holdback heuristics) shared by every frontend.
//! * [`reasoning`] — shared reasoning-segment contract (sentinel and escapes).
//!
//! The public surface is re-exported here so consumers only ever name
//! `crate::markdown::*`.

pub mod blocks;
pub mod dialect;
pub mod document;
pub mod highlight;
pub mod math;
pub mod plain;
pub mod reasoning;
pub mod render_options;
pub mod stream;
pub mod styled;

pub use dialect::{has_reference_definition, has_table_risk, parser_options};

pub use stream::{
    settled_boundary, MarkdownFrame, MarkdownStream, ParseMetrics, DEFAULT_MAX_SOURCE_BYTES,
    DEFAULT_PARSE_INTERVAL_MS,
};

pub use plain::render_plain_text;
pub use styled::{render_styled_lines, render_styled_lines_animated};

pub use blocks::ends_with_blank_line;
