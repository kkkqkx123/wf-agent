//! Syntax highlighting backed by `wf-syntax` (tree-sitter).
//!
//! This module preserves the historical `tui-markdown` highlight API while
//! delegating implementation to the shared syntax crate.

pub use wf_syntax::*;
