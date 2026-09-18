//! TUI debug utilities: diff recording, key receiver, animation helpers.
//!
//! All items in this module are gated behind the `diff-record` feature flag
//! (for the recorder) or are standalone example helpers that live in
//! `examples/`.

#[cfg(feature = "diff-record")]
pub mod diff_recorder;

#[cfg(feature = "diff-record")]
pub use diff_recorder::DiffRecorderBackend;
