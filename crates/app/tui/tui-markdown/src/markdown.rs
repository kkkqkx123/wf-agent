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
//! * [`document`] — backend-neutral semantic model ([`Document`],
//!   [`StyleRole`], [`StyledSpan`]) shared by every frontend.
//! * [`reasoning`] — shared reasoning-segment contract (sentinel and escapes).
//!
//! The public surface is re-exported here so consumers only ever name
//! `crate::markdown::*`.

pub mod blocks;
pub mod document;
pub mod plain;
pub mod reasoning;
pub mod stream;
pub mod styled;

pub use stream::{
    MarkdownFrame, MarkdownStream, ParseMetrics, DEFAULT_MAX_SOURCE_BYTES,
    DEFAULT_PARSE_INTERVAL_MS,
};

pub use plain::render_plain_text;
pub use styled::{render_styled_lines, render_styled_lines_animated};

pub use blocks::ends_with_blank_line;
