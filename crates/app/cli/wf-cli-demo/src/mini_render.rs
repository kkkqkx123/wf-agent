//! Mini rendering plumbing for demos: markdown sample content plus
//! re-exports of the real `wf-mini` rendering primitives.
//!
//! Rendering logic is re-exported from the `wf-mini` library target so
//! demos exercise the actual implementation instead of a mirrored copy.
//! Only demo-specific content (the sample document) lives here.

pub use wf_mini::output::AppendWriter;
pub use wf_mini::render::{
    has_reference_definition, render_plain, render_styled, strip_terminal_escapes, StreamRenderer,
};

/// Sample document covering heading, emphasis, code, link, fence, two-line
/// quote, rule, image, inline HTML, strikethrough, table and task list.
/// Every construct maps to a distinct `render_styled`/`render_plain` branch
/// reachable with the canonical parser dialect (`tui-markdown-dialect`).
pub const SAMPLE_MARKDOWN: &str = "# Fix login\n\nRetry with **backoff** and *jitter*, see `login()` and [docs](https://example.com).\n\n```rust\nfn login() {}\n```\n\n> quoted note\n> continued line\n\n---\n\n![logo](https://example.com/logo.png)\n\n<span>inline html dropped</span>\n\n~~strikethrough text~~ removed\n\n| area | status |\n| --- | --- |\n| auth | fixed |\n\n- [x] retry with backoff\n- [ ] alert on failure\n";
