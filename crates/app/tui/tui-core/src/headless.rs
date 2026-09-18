//! Headless summary renderer: `ExecutionStreamEvent` → plain text.
//!
//! [`HeadlessRenderer`] composes [`SessionReducer`] (footer state + the
//! reducer product used by the TUI inline scrollback) and [`MarkdownStream`]
//! (streaming markdown) into a headless summary renderer that turns the
//! execution event stream into [`HeadlessDelta`]:
//!
//! * assistant text → the markdown pipeline: settled blocks render to plain
//!   text, the in-flight block streams **complete lines only** (soft breaks
//!   keep `\n`);
//! * tool lifecycle → `diag` (stderr), matching the `▲/✓/✗` diagnostic lines;
//! * iteration boundaries flush the pending markdown tail (and start a fresh
//!   block for the next iteration).
//!
//! The same events are fed to the reducer, so [`HeadlessRenderer::footer`]
//! reflects the live session state, and the same-source invariant test (in
//! `tui-render`) asserts that the headless stdout text matches what the TUI
//! inline scrollback (`HistoryLine`) renders from the same reducer commits.
//!
//! The production CLIs (`wf-mini`, `wf-headless`) stream raw markdown
//! verbatim by design, so this renderer is the markdown-stripped summary
//! form: the kernel every display that opts into plain-text output shares.
//!
//! This is a pure-data renderer: no TTY, no IO, no wall clock (tool lines
//! carry no duration), so outputs are deterministic and testable.

use crate::reducer::{FooterState, SessionReducer};
use tui_markdown::markdown::{ends_with_blank_line, render_plain_text, MarkdownStream};
use wf_api::infra::stream::ExecutionStreamEvent;

/// One output delta from the headless renderer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadlessDelta {
    /// New plain-text content to append to stdout (never re-emitted).
    pub stdout: String,
    /// New diagnostic lines (tool lifecycle) to send to stderr.
    pub diag: Vec<String>,
    /// Whether any business output has been produced so far.
    pub had_output: bool,
}

/// Headless summary renderer: reducer + streaming markdown → [`HeadlessDelta`].
pub struct HeadlessRenderer {
    reducer: SessionReducer,
    markdown: MarkdownStream,
    /// Plain-text output already delivered to stdout (for incremental diff).
    emitted_stdout: String,
    had_output: bool,
}

impl HeadlessRenderer {
    pub fn new(execution_id: impl Into<String>) -> Self {
        Self {
            reducer: SessionReducer::new(execution_id),
            markdown: MarkdownStream::default(),
            emitted_stdout: String::new(),
            had_output: false,
        }
    }

    /// Feed one execution stream event and return the output it produced.
    ///
    /// The event is also reduced into the footer state so
    /// [`HeadlessRenderer::footer`] tracks the live session.
    pub fn on_event(&mut self, event: &ExecutionStreamEvent) -> HeadlessDelta {
        self.reducer.push_batch(std::slice::from_ref(event));
        match event {
            ExecutionStreamEvent::LlmDelta { content } => {
                self.markdown.push(content);
                HeadlessDelta {
                    stdout: self.sync_stdout(true),
                    diag: Vec::new(),
                    had_output: self.had_output,
                }
            }
            // Iteration boundaries flush the pending markdown and reset the
            // block: each iteration is an independent output block (the
            // reducer emits one AssistantText commit per iteration too).
            ExecutionStreamEvent::IterationStart { .. }
            | ExecutionStreamEvent::IterationEnd { .. } => {
                let stdout = self.flush_iteration();
                HeadlessDelta {
                    stdout,
                    diag: Vec::new(),
                    had_output: self.had_output,
                }
            }
            ExecutionStreamEvent::ToolStart { tool_name, .. } => {
                self.had_output = true;
                HeadlessDelta {
                    stdout: String::new(),
                    diag: vec![format!("▲ {tool_name}")],
                    had_output: true,
                }
            }
            ExecutionStreamEvent::ToolEnd {
                tool_name, success, ..
            } => {
                self.had_output = true;
                let diag = if *success {
                    format!("✓ {tool_name}")
                } else {
                    format!("✗ {tool_name}")
                };
                HeadlessDelta {
                    stdout: String::new(),
                    diag: vec![diag],
                    had_output: true,
                }
            }
            // Engine lifecycle / terminal / interruption events carry no
            // incremental output here; the reducer already folds them into
            // the footer.
            _ => HeadlessDelta {
                stdout: String::new(),
                diag: Vec::new(),
                had_output: self.had_output,
            },
        }
    }

    /// Close the stream: flush everything remaining and reset the markdown
    /// block. Unlike an iteration flush, `finish` does **not** force a
    /// trailing newline so the accumulated stdout equals the rendered text.
    pub fn finish(&mut self) -> HeadlessDelta {
        let stdout = self.sync_stdout(false);
        self.markdown.finish();
        self.emitted_stdout.clear();
        HeadlessDelta {
            stdout,
            diag: Vec::new(),
            had_output: self.had_output,
        }
    }

    /// Live footer state (phase / iteration / active tools / errors).
    pub fn footer(&self) -> &FooterState {
        self.reducer.footer()
    }

    /// Emit the newly rendered plain-text lines since the last call.
    ///
    /// `hold_back` keeps the final in-flight line out of the output until a
    /// newline settles it ("complete lines only"): when the source ends with
    /// a newline the last line is complete and safe to emit; otherwise the
    /// rendered text is truncated at its last newline. Diffing against
    /// `emitted_stdout` never re-emits delivered bytes; if a settled line
    /// retroactively changed (e.g. the final render collapses a trailing
    /// newline) only the extension of the common prefix is emitted.
    fn sync_stdout(&mut self, hold_back: bool) -> String {
        let src = self.markdown.source();
        if src.is_empty() {
            return String::new();
        }
        let mut rendered = render_plain_text(src);
        // pulldown-cmark folds a paragraph's trailing terminator newline into
        // the paragraph end (no `SoftBreak`), so a source ending in a single
        // newline would render without it. A single trailing newline is a
        // real in-flight line break ("complete lines only"): restore it so
        // the line emits whole. A blank-line terminator (two newlines) is a
        // block settlement — blank lines collapse and get no trailing NL.
        if src.ends_with('\n')
            && !rendered.is_empty()
            && !rendered.ends_with('\n')
            && !ends_with_blank_line(src)
        {
            rendered.push('\n');
        }
        let target = if hold_back && !src.ends_with('\n') {
            match rendered.rfind('\n') {
                Some(pos) => rendered[..=pos].to_string(),
                None => String::new(),
            }
        } else {
            rendered
        };
        let new = if target.starts_with(&self.emitted_stdout) {
            target[self.emitted_stdout.len()..].to_string()
        } else {
            let common = common_prefix_len(&self.emitted_stdout, &target);
            target[common..].to_string()
        };
        self.emitted_stdout = target;
        if !new.is_empty() {
            self.had_output = true;
        }
        new
    }

    /// Flush the pending markdown block as one output block: emit everything
    /// (no hold-back), terminate the block with a newline so consecutive
    /// iteration outputs do not run together, then reset the stream.
    fn flush_iteration(&mut self) -> String {
        let mut stdout = self.sync_stdout(false);
        if !stdout.is_empty() && !stdout.ends_with('\n') {
            stdout.push('\n');
        }
        self.markdown.finish();
        self.emitted_stdout.clear();
        stdout
    }
}

/// Byte length of the common prefix of `a` and `b`, aligned to a char
/// boundary (so the suffix split never lands mid-codepoint).
fn common_prefix_len(a: &str, b: &str) -> usize {
    let bytes = a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count();
    let mut len = bytes;
    while len > 0 && !b.is_char_boundary(len) {
        len -= 1;
    }
    len
}

#[cfg(test)]
mod tests {
    use super::*;

    fn delta(text: &str) -> ExecutionStreamEvent {
        ExecutionStreamEvent::LlmDelta {
            content: text.to_string(),
        }
    }

    #[test]
    fn empty_stream_produces_no_output() {
        let mut renderer = HeadlessRenderer::new("exec-1");
        assert_eq!(renderer.finish().stdout, "");
        assert!(!renderer.finish().had_output);
    }

    #[test]
    fn streaming_paragraph_emits_complete_lines_only() {
        let mut renderer = HeadlessRenderer::new("exec-1");
        // In-flight (no newline): nothing is emitted.
        assert_eq!(renderer.on_event(&delta("Hello, ")).stdout, "");
        // The newline completes the line → emitted.
        assert_eq!(
            renderer.on_event(&delta("world!\n")).stdout,
            "Hello, world!\n"
        );
        // The next in-flight line stays hidden until it ends.
        assert_eq!(renderer.on_event(&delta("more")).stdout, "");
        // finish() flushes the incomplete tail.
        assert_eq!(renderer.finish().stdout, "more");
    }

    #[test]
    fn finish_emits_incomplete_last_line() {
        let mut renderer = HeadlessRenderer::new("exec-1");
        assert_eq!(renderer.on_event(&delta("hi")).stdout, "");
        assert_eq!(renderer.finish().stdout, "hi");
    }

    #[test]
    fn soft_breaks_stream_as_lines() {
        let mut renderer = HeadlessRenderer::new("exec-1");
        assert_eq!(renderer.on_event(&delta("line1\n")).stdout, "line1\n");
        assert_eq!(renderer.on_event(&delta("line2\n")).stdout, "line2\n");
        assert_eq!(renderer.finish().stdout, "");
    }

    #[test]
    fn blank_line_settles_paragraph_without_trailing_newline() {
        let mut renderer = HeadlessRenderer::new("exec-1");
        // A single push carrying the settling blank line renders the settled
        // paragraph immediately (blank lines are collapsed by the renderer).
        assert_eq!(renderer.on_event(&delta("a\n\n")).stdout, "a");
        assert_eq!(renderer.finish().stdout, "");
    }

    #[test]
    fn incremental_delivery_never_reemits_stdout() {
        let mut renderer = HeadlessRenderer::new("exec-1");
        let mut seen = String::new();
        for chunk in ["hello ", "world\n", "again", "\n"] {
            let delta = renderer.on_event(&delta(chunk));
            assert!(delta.diag.is_empty());
            seen.push_str(&delta.stdout);
        }
        seen.push_str(&renderer.finish().stdout);
        assert_eq!(seen, "hello world\nagain\n");
    }

    #[test]
    fn fenced_code_block_renders_content_verbatim() {
        let mut renderer = HeadlessRenderer::new("exec-1");
        let mut stdout = String::new();
        stdout.push_str(&renderer.on_event(&delta("```rust\n")).stdout);
        stdout.push_str(&renderer.on_event(&delta("fn main() {\n")).stdout);
        stdout.push_str(&renderer.on_event(&delta("    println!();\n")).stdout);
        stdout.push_str(&renderer.on_event(&delta("}\n```")).stdout);
        stdout.push_str(&renderer.finish().stdout);
        assert_eq!(stdout, "fn main() {\n    println!();\n}\n");
    }

    #[test]
    fn tool_lifecycle_goes_to_diag() {
        let mut renderer = HeadlessRenderer::new("exec-1");
        let start = renderer.on_event(&ExecutionStreamEvent::ToolStart {
            tool_call_id: "t1".into(),
            tool_name: "bash".into(),
        });
        assert_eq!(start.stdout, "");
        assert_eq!(start.diag, vec!["▲ bash"]);
        assert!(start.had_output);

        let end = renderer.on_event(&ExecutionStreamEvent::ToolEnd {
            tool_call_id: "t1".into(),
            tool_name: "bash".into(),
            success: true,
            result: String::new(),
        });
        assert_eq!(end.diag, vec!["✓ bash"]);

        let fail = renderer.on_event(&ExecutionStreamEvent::ToolEnd {
            tool_call_id: "t2".into(),
            tool_name: "write_file".into(),
            success: false,
            result: String::new(),
        });
        assert_eq!(fail.diag, vec!["✗ write_file"]);
    }

    #[test]
    fn had_output_tracks_any_emitted_output() {
        let mut renderer = HeadlessRenderer::new("exec-1");
        // In-flight text produces no visible output yet.
        assert!(!renderer.on_event(&delta("partial")).had_output);
        // A completed line is visible output.
        assert!(renderer.on_event(&delta("!\n")).had_output);
        assert!(renderer.footer().phase == crate::reducer::Phase::Streaming);
    }

    #[test]
    fn iteration_boundary_flushes_pending_text_as_a_block() {
        let mut renderer = HeadlessRenderer::new("exec-1");
        renderer.on_event(&delta("partial"));
        let flushed = renderer.on_event(&ExecutionStreamEvent::IterationEnd {
            iteration: 1,
            message_count: 0,
            array_version: 0,
        });
        assert_eq!(flushed.stdout, "partial\n");
        // The next iteration starts a fresh markdown block.
        assert_eq!(renderer.on_event(&delta("next")).stdout, "");
        assert_eq!(renderer.finish().stdout, "next");
    }

    #[test]
    fn completed_returns_footer_to_idle() {
        let mut renderer = HeadlessRenderer::new("exec-1");
        renderer.on_event(&delta("done"));
        renderer.on_event(&ExecutionStreamEvent::Completed {
            result: serde_json::Value::Null,
            iterations: 3,
        });
        assert_eq!(renderer.footer().phase, crate::reducer::Phase::Idle);
        assert_eq!(renderer.footer().iteration, 0);
    }
}
