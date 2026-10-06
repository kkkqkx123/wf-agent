//! Session rendering: delta buffering and agent event rendering.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::error::CliResult;
use crate::output::{OutputFormat, OutputMessage, OutputSink};
use crate::run_diag::DiagWriter;
use wf_api::infra::stream::ExecutionStreamEvent;

// ── delta line buffering ──────────────────────────────────────────

/// Merges LLM deltas before they hit stdout: complete lines flush
/// immediately, partial tails wait for more input or a threshold.
#[derive(Debug)]
pub struct DeltaBuffer {
    buf: String,
    max_bytes: usize,
}

impl Default for DeltaBuffer {
    fn default() -> Self {
        Self::new(8 * 1024)
    }
}

impl DeltaBuffer {
    pub fn new(max_bytes: usize) -> Self {
        Self {
            buf: String::new(),
            max_bytes,
        }
    }

    /// Append a delta and return the ready-to-write segment (text up to the
    /// last newline, or the whole buffer once it exceeds the threshold).
    pub fn push(&mut self, delta: &str) -> String {
        self.buf.push_str(delta);
        if self.buf.contains('\n') {
            let split = self.buf.rfind('\n').expect("checked above") + 1;
            let ready = self.buf[..split].to_string();
            self.buf.replace_range(..split, "");
            ready
        } else if self.buf.len() >= self.max_bytes {
            std::mem::take(&mut self.buf)
        } else {
            String::new()
        }
    }

    /// Flush whatever remains (iteration/terminal boundary).
    pub fn take_remaining(&mut self) -> String {
        std::mem::take(&mut self.buf)
    }
}

// ── session rendering ────────────────────────────────────────────────

/// Renders agent events into the main sink (business output) and the
/// diagnostics channel (tool lifecycle).
pub(crate) struct SessionRenderer<'a> {
    sink: &'a mut dyn OutputSink,
    format: OutputFormat,
    delta_buf: DeltaBuffer,
    /// Full assistant text of the current iteration (structured formats
    /// emit one message record per iteration).
    iteration_text: String,
    /// True when an emitted chunk did not end with a newline (the next
    /// diagnostic or summary must terminate the line first).
    line_pending: bool,
    saw_text: bool,
    tool_started_at: HashMap<String, Instant>,
    pub(crate) had_output: bool,
}

impl<'a> SessionRenderer<'a> {
    pub(crate) fn new(sink: &'a mut dyn OutputSink, format: OutputFormat) -> Self {
        Self {
            sink,
            format,
            delta_buf: DeltaBuffer::default(),
            iteration_text: String::new(),
            line_pending: false,
            saw_text: false,
            tool_started_at: HashMap::new(),
            had_output: false,
        }
    }

    pub(crate) fn on_event(
        &mut self,
        event: &ExecutionStreamEvent,
        diag: &Arc<Mutex<DiagWriter>>,
    ) -> CliResult<()> {
        match event {
            // Engine lifecycle events carry no execution progress payload
            // for a headless run; skip them.
            ExecutionStreamEvent::Engine(_) => return Ok(()),
            // Terminal and interruption events are handled by the run
            // loop, not the renderer.
            ExecutionStreamEvent::Completed { .. }
            | ExecutionStreamEvent::Failed { .. }
            | ExecutionStreamEvent::Interrupted { .. } => return Ok(()),
            ExecutionStreamEvent::LlmDelta { content } => {
                self.had_output = true;
                self.saw_text = true;
                self.iteration_text.push_str(content);
                if self.format == OutputFormat::Text {
                    let ready = self.delta_buf.push(content);
                    if !ready.is_empty() {
                        self.sink.write_chunk(&ready)?;
                        self.line_pending = !ready.ends_with('\n');
                    }
                }
            }
            ExecutionStreamEvent::ToolStart {
                tool_call_id,
                tool_name,
            } => {
                self.had_output = true;
                self.tool_started_at
                    .insert(tool_call_id.clone(), Instant::now());
                let mut diag = wf_common::lock::lock_ok(diag.lock());
                let _ = diag.line(&format!("▲ {tool_name}"));
            }
            ExecutionStreamEvent::ToolEnd {
                tool_call_id,
                tool_name,
                success,
                error,
                ..
            } => {
                let elapsed = self
                    .tool_started_at
                    .remove(tool_call_id)
                    .map(|started| started.elapsed());
                let line = match (success, elapsed) {
                    (true, Some(d)) => format!("✓ {tool_name} ({}ms)", d.as_millis()),
                    (true, None) => format!("✓ {tool_name}"),
                    (false, _) => match error {
                        Some(reason) => format!("✗ {tool_name}: {reason}"),
                        None => format!("✗ {tool_name}"),
                    },
                };
                let mut diag = wf_common::lock::lock_ok(diag.lock());
                if *success {
                    let _ = diag.ok(&line);
                } else {
                    let _ = diag.err(&line);
                }
            }
            ExecutionStreamEvent::IterationEnd { .. } => self.flush_iteration()?,
            ExecutionStreamEvent::IterationStart { .. } => {}
            // Reasoning / usage / sub-agent lifecycle carry no business or
            // diagnostics output in the headless renderer; the reducer still
            // folds them into the footer snapshot.
            ExecutionStreamEvent::ReasoningDelta { .. }
            | ExecutionStreamEvent::Usage { .. }
            | ExecutionStreamEvent::SubAgentStarted { .. }
            | ExecutionStreamEvent::SubAgentEnded { .. } => return Ok(()),
        }
        Ok(())
    }

    /// Commit the current iteration: text mode flushes the delta tail and
    /// terminates the line; structured formats emit one assistant record.
    pub(crate) fn flush_iteration(&mut self) -> CliResult<()> {
        if self.format == OutputFormat::Text {
            let rest = self.delta_buf.take_remaining();
            if !rest.is_empty() {
                self.sink.write_chunk(&rest)?;
                self.line_pending = !rest.ends_with('\n');
            }
            if self.line_pending {
                self.sink.write_chunk("\n")?;
                self.line_pending = false;
            }
        } else if !self.iteration_text.is_empty() && !self.format.is_silent() {
            let text = std::mem::take(&mut self.iteration_text);
            self.sink
                .write_message(&OutputMessage::new("assistant", text))?;
        } else {
            self.iteration_text.clear();
        }
        self.saw_text = false;
        Ok(())
    }

    /// Ensure the stream ends on a fresh line (text mode).
    pub(crate) fn finish(&mut self) -> CliResult<()> {
        if self.line_pending {
            self.sink.write_chunk("\n")?;
            self.line_pending = false;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::MemorySink;

    fn shared_diag() -> Arc<Mutex<DiagWriter>> {
        Arc::new(Mutex::new(DiagWriter::buffer()))
    }

    fn diag_text(diag: &Arc<Mutex<DiagWriter>>) -> String {
        wf_common::lock::lock_ok(diag.lock()).snapshot()
    }

    #[test]
    fn delta_buffer_flushes_on_newline_and_threshold() {
        let mut buf = DeltaBuffer::new(8);

        // No newline yet: buffered.
        assert_eq!(buf.push("hel"), "");
        assert_eq!(buf.push("lo"), "");

        // Newline flushes the complete line, keeps the tail.
        assert_eq!(buf.push(" wo\nrld"), "hello wo\n");
        assert_eq!(buf.take_remaining(), "rld");

        // Threshold flushes without a newline.
        let mut buf = DeltaBuffer::new(4);
        assert_eq!(buf.push("abcdef"), "abcdef");
        assert_eq!(buf.take_remaining(), "");
    }

    #[test]
    fn text_renderer_streams_deltas_and_lines() {
        let mut sink = MemorySink::new();
        let format = OutputFormat::Text;
        let diag = shared_diag();
        {
            let mut renderer = SessionRenderer::new(&mut sink, format);
            renderer
                .on_event(
                    &ExecutionStreamEvent::LlmDelta {
                        content: "hello\nbig ".into(),
                    },
                    &diag,
                )
                .unwrap();
            renderer
                .on_event(
                    &ExecutionStreamEvent::LlmDelta {
                        content: "world".into(),
                    },
                    &diag,
                )
                .unwrap();
            // Iteration boundary flushes the pending tail + newline.
            renderer
                .on_event(
                    &ExecutionStreamEvent::IterationEnd {
                        iteration: 1,
                        message_count: 0,
                        array_version: 0,
                    },
                    &diag,
                )
                .unwrap();
        }
        assert_eq!(sink.text(), "hello\nbig world\n");
    }

    #[test]
    fn text_renderer_marks_tool_lifecycle_on_diag() {
        let mut sink = MemorySink::new();
        let diag = shared_diag();
        {
            let mut renderer = SessionRenderer::new(&mut sink, OutputFormat::Text);
            renderer
                .on_event(
                    &ExecutionStreamEvent::ToolStart {
                        tool_call_id: "t1".into(),
                        tool_name: "read_file".into(),
                    },
                    &diag,
                )
                .unwrap();
            renderer
                .on_event(
                    &ExecutionStreamEvent::ToolEnd {
                        tool_call_id: "t1".into(),
                        tool_name: "read_file".into(),
                        success: true,
                        result: String::new(),
                        error: None,
                    },
                    &diag,
                )
                .unwrap();
            renderer
                .on_event(
                    &ExecutionStreamEvent::ToolEnd {
                        tool_call_id: "t2".into(),
                        tool_name: "write_file".into(),
                        success: false,
                        result: String::new(),
                        error: None,
                    },
                    &diag,
                )
                .unwrap();
        }
        assert_eq!(sink.text(), "");
        let text = diag_text(&diag);
        assert!(text.contains("▲ read_file"), "{text}");
        assert!(text.contains("✓ read_file"), "{text}");
        assert!(text.contains("✗ write_file"), "{text}");
    }

    #[test]
    fn structured_renderer_emits_one_record_per_iteration() {
        let mut sink = MemorySink::new();
        let diag = shared_diag();
        {
            let mut renderer = SessionRenderer::new(&mut sink, OutputFormat::Json);
            for (chunk, iteration) in [
                ("part ", 1u32),
                ("one", 1),
                // Iteration boundary flushes the pending tail.
                ("two", 2),
            ] {
                renderer
                    .on_event(
                        &ExecutionStreamEvent::LlmDelta {
                            content: chunk.into(),
                        },
                        &diag,
                    )
                    .unwrap();
                if chunk == "one" || chunk == "two" {
                    renderer
                        .on_event(
                            &ExecutionStreamEvent::IterationEnd {
                                iteration,
                                message_count: 0,
                                array_version: 0,
                            },
                            &diag,
                        )
                        .unwrap();
                }
            }
        }
        let messages = sink.messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content, "part one");
        assert_eq!(messages[1].content, "two");
    }
}
