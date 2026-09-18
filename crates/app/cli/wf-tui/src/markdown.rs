//! Markdown rendering for the TUI.
//!
//! This module groups four responsibilities, each in its own submodule:
//!
//! * [`stream`] — the incremental streaming state machine ([`MarkdownStream`],
//!   [`MarkdownFrame`], [`ParseMetrics`]) that splits a growing source into a
//!   settled (committed) region and an in-flight (streaming) region.
//! * [`blocks`] — stateless block-structure analysis (fences, tables,
//!   reference definitions, blank-line settlement) used to compute the split.
//! * [`plain`] — whole-source Markdown → plain text ([`render_plain_text`]).
//! * [`styled`] — whole-source Markdown → styled ratatui `Line`s
//!   ([`render_styled_lines`], [`render_styled_lines_animated`]).
//!
//! The public surface is re-exported here so consumers only ever name
//! `crate::markdown::*`.

pub mod blocks;
pub mod plain;
pub mod stream;
pub mod styled;

pub use stream::{
    MarkdownFrame, MarkdownStream, ParseMetrics, DEFAULT_MAX_SOURCE_BYTES,
    DEFAULT_PARSE_INTERVAL_MS,
};

pub use plain::render_plain_text;
pub use styled::{render_styled_lines, render_styled_lines_animated};

pub(crate) use blocks::ends_with_blank_line;
