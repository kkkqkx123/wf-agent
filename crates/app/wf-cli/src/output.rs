//! Output layer: format layer × sink (target) layer.
//!
//! Two orthogonal dimensions:
//!   - Format: how content is rendered (`text` / `json` / `jsonl` / `silent`).
//!   - Sink:   where content goes (stdout / file / memory / tee fan-out).
//!
//! Business code only calls [`OutputSink`] methods and never decides the
//! destination; memory-backed sinks serve the interactive forms (mini / full
//! TUI).

use std::io::{self, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use serde::Serialize;

use crate::error::{CliError, CliResult};

/// Minimal ANSI styling used when color is enabled (text format only).
const RESET: &str = "\x1b[0m";
const BOLD_CYAN: &str = "\x1b[1;36m";

/// Output format for the format layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
#[clap(rename_all = "lower")]
pub enum OutputFormat {
    /// Human-readable text stream.
    #[default]
    Text,
    /// Single JSON object per message; a summary envelope closes the output.
    Json,
    /// One JSON record per line; no envelope (pipe/file friendly).
    #[value(name = "jsonl", alias = "jsonlines")]
    JsonLines,
    /// No output; the process exit code carries the result.
    Silent,
}

impl OutputFormat {
    /// Whether this format suppresses all output.
    pub fn is_silent(&self) -> bool {
        matches!(self, Self::Silent)
    }
}

/// Lightweight structured message written through a sink. Decoupled from the
/// domain message types so the sink layer stays independent; adapters convert
/// domain messages into this shape at the boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Map<String, serde_json::Value>>,
}

impl OutputMessage {
    pub fn new(role: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
            meta: None,
        }
    }

    pub fn with_meta(
        role: impl Into<String>,
        content: impl Into<String>,
        meta: serde_json::Map<String, serde_json::Value>,
    ) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
            meta: Some(meta),
        }
    }
}

/// Command result envelope:
/// `{success, type, entity, data, message, timestamp}`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputEnvelope {
    pub success: bool,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity: Option<String>,
    pub data: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub timestamp: i64,
}

impl OutputEnvelope {
    /// Build a success envelope.
    pub fn success(kind: impl Into<String>, data: serde_json::Value) -> Self {
        Self {
            success: true,
            kind: kind.into(),
            entity: None,
            data,
            message: None,
            timestamp: wf_common::now(),
        }
    }

    /// Build a failure envelope.
    pub fn failure(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            success: false,
            kind: kind.into(),
            entity: None,
            data: serde_json::Value::Null,
            message: Some(message.into()),
            timestamp: wf_common::now(),
        }
    }

    pub fn with_entity(mut self, entity: impl Into<String>) -> Self {
        self.entity = Some(entity.into());
        self
    }

    /// Render the envelope in the requested format. `text` produces a short
    /// human-readable line; `json` a single JSON object line; `jsonl` and
    /// `silent` produce nothing (jsonl is a pure record stream).
    pub fn render(&self, format: OutputFormat) -> Option<String> {
        match format {
            OutputFormat::Text => {
                let status = if self.success { "ok" } else { "failed" };
                let msg =
                    self.message
                        .as_deref()
                        .unwrap_or(if self.success { "done" } else { "error" });
                Some(format!("[{status}] {}: {msg}", self.kind))
            }
            OutputFormat::Json => serde_json::to_string(self).ok(),
            OutputFormat::JsonLines | OutputFormat::Silent => None,
        }
    }
}

/// A structured event recorded by [`MemorySink`] for assertions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SinkEvent {
    Text(String),
    Message(OutputMessage),
    Chunk(String),
    Raw(String),
    Flush,
}

/// Generic output slot. Business code writes through this trait and never
/// decides whether the destination is stdout, a file, memory or the TUI
/// state. Implementations must be object-safe and `Send` so sinks can be
/// shared across async task boundaries.
pub trait OutputSink {
    /// Write a complete text line/paragraph (format-aware: structured
    /// formats drop free text).
    fn write_text(&mut self, text: &str) -> io::Result<()>;
    /// Write a structured message (conversation message, tool result summary).
    fn write_message(&mut self, msg: &OutputMessage) -> io::Result<()>;
    /// Append an incremental LLM text delta (streaming).
    fn write_chunk(&mut self, chunk: &str) -> io::Result<()>;
    /// Write a pre-rendered record (envelope line, JSON object) bypassing the
    /// format filter. Callers are responsible for the format-correct bytes.
    fn write_raw(&mut self, record: &str) -> io::Result<()>;
    /// Flush any buffered bytes to the destination.
    fn flush(&mut self) -> io::Result<()>;
}

/// Underlying byte destination of a [`HeadlessFileSink`]. The `Bytes` variant
/// backs the in-memory buffer used by tests; everything else is a real writer.
enum SinkWriter {
    Bytes(Vec<u8>),
    Dyn(Box<dyn Write + Send>),
}

impl SinkWriter {
    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        match self {
            SinkWriter::Bytes(v) => {
                v.extend_from_slice(buf);
                Ok(())
            }
            SinkWriter::Dyn(w) => w.write_all(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            SinkWriter::Bytes(_) => Ok(()),
            SinkWriter::Dyn(w) => w.flush(),
        }
    }
}

/// Sink writing to a generic byte writer: stdout, a file, a pipe or an
/// in-memory buffer. ANSI color is enabled only when the format is `text`
/// and color was requested; structured formats always write clean bytes.
pub struct HeadlessFileSink {
    writer: SinkWriter,
    format: OutputFormat,
    color: bool,
    /// Flush after every write (streaming to a pipe/TTY). File outputs flush
    /// less frequently for throughput.
    flush_every_write: bool,
}

impl HeadlessFileSink {
    /// Wrap an arbitrary writer.
    pub fn new(
        writer: Box<dyn Write + Send>,
        format: OutputFormat,
        color: bool,
        flush_every_write: bool,
    ) -> Self {
        Self {
            writer: SinkWriter::Dyn(writer),
            format,
            color: color && format == OutputFormat::Text,
            flush_every_write,
        }
    }

    /// Sink bound to process stdout with real-time flushing (pipe friendly).
    pub fn stdout(format: OutputFormat, color: bool) -> Self {
        Self::new(Box::new(io::stdout()), format, color, true)
    }

    /// Sink bound to a file; flushed on explicit `flush` calls.
    pub fn file(path: &Path, format: OutputFormat, color: bool) -> CliResult<Self> {
        let writer = std::fs::File::create(path).map_err(|err| {
            CliError::Configuration(format!(
                "cannot open output file '{}': {err}",
                path.display()
            ))
        })?;
        Ok(Self::new(Box::new(writer), format, color, false))
    }

    /// Sink bound to an in-memory buffer (tests).
    pub fn buffer(buf: Vec<u8>, format: OutputFormat, color: bool) -> Self {
        Self {
            writer: SinkWriter::Bytes(buf),
            format,
            color: color && format == OutputFormat::Text,
            flush_every_write: false,
        }
    }

    /// Extract the accumulated bytes. Returns an empty vec for sinks backed by
    /// a real writer (stdout / file). Intended for test assertions.
    pub fn into_bytes(self) -> Vec<u8> {
        match self.writer {
            SinkWriter::Bytes(v) => v,
            SinkWriter::Dyn(_) => Vec::new(),
        }
    }

    fn write_line(&mut self, line: &str) -> io::Result<()> {
        self.writer.write_all(line.as_bytes())?;
        self.writer.write_all(b"\n")?;
        if self.flush_every_write {
            self.writer.flush()?;
        }
        Ok(())
    }
}

impl OutputSink for HeadlessFileSink {
    fn write_text(&mut self, text: &str) -> io::Result<()> {
        if self.format != OutputFormat::Text {
            // Structured formats expect messages/records, not free text.
            return Ok(());
        }
        self.write_line(text)
    }

    fn write_message(&mut self, msg: &OutputMessage) -> io::Result<()> {
        match self.format {
            OutputFormat::Text => {
                let role = if self.color {
                    format!("{BOLD_CYAN}{}{RESET}", msg.role)
                } else {
                    msg.role.clone()
                };
                self.write_line(&format!("{role}: {}", msg.content))
            }
            OutputFormat::Json | OutputFormat::JsonLines => {
                let line = serde_json::to_string(msg)
                    .map_err(|err| io::Error::other(format!("message serialization: {err}")))?;
                self.write_line(&line)
            }
            OutputFormat::Silent => Ok(()),
        }
    }

    fn write_chunk(&mut self, chunk: &str) -> io::Result<()> {
        if self.format != OutputFormat::Text {
            // Deltas are a text-stream concept; structured formats receive
            // complete messages through `write_message` instead.
            return Ok(());
        }
        self.writer.write_all(chunk.as_bytes())?;
        if self.flush_every_write {
            self.writer.flush()?;
        }
        Ok(())
    }

    fn write_raw(&mut self, record: &str) -> io::Result<()> {
        self.write_line(record)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

/// In-memory sink recording every call for assertions. Lets tests validate
/// the business output contract without a real terminal.
#[derive(Debug, Default)]
pub struct MemorySink {
    events: Vec<SinkEvent>,
}

impl MemorySink {
    pub fn new() -> Self {
        Self::default()
    }

    /// Concatenated text and chunk payloads.
    pub fn text(&self) -> String {
        self.events
            .iter()
            .filter_map(|e| match e {
                SinkEvent::Text(t) => Some(t.clone()),
                SinkEvent::Chunk(c) => Some(c.clone()),
                _ => None,
            })
            .collect()
    }

    /// All pre-rendered raw records written.
    pub fn raw(&self) -> Vec<&str> {
        self.events
            .iter()
            .filter_map(|e| match e {
                SinkEvent::Raw(r) => Some(r.as_str()),
                _ => None,
            })
            .collect()
    }

    /// All structured messages written.
    pub fn messages(&self) -> Vec<&OutputMessage> {
        self.events
            .iter()
            .filter_map(|e| match e {
                SinkEvent::Message(m) => Some(m),
                _ => None,
            })
            .collect()
    }

    /// Number of flush calls.
    pub fn flush_count(&self) -> usize {
        self.events
            .iter()
            .filter(|e| **e == SinkEvent::Flush)
            .count()
    }
}

impl OutputSink for MemorySink {
    fn write_text(&mut self, text: &str) -> io::Result<()> {
        self.events.push(SinkEvent::Text(text.to_string()));
        Ok(())
    }

    fn write_message(&mut self, msg: &OutputMessage) -> io::Result<()> {
        self.events.push(SinkEvent::Message(msg.clone()));
        Ok(())
    }

    fn write_chunk(&mut self, chunk: &str) -> io::Result<()> {
        self.events.push(SinkEvent::Chunk(chunk.to_string()));
        Ok(())
    }

    fn write_raw(&mut self, record: &str) -> io::Result<()> {
        self.events.push(SinkEvent::Raw(record.to_string()));
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.events.push(SinkEvent::Flush);
        Ok(())
    }
}

/// Fan-out sink forwarding every call to all child sinks (interactive +
/// file capture, stderr + stdout, etc.).
pub struct TeeSink {
    sinks: Vec<Box<dyn OutputSink + Send>>,
}

impl TeeSink {
    pub fn new(sinks: Vec<Box<dyn OutputSink + Send>>) -> Self {
        Self { sinks }
    }

    /// Append a child sink at runtime.
    pub fn push(&mut self, sink: Box<dyn OutputSink + Send>) {
        self.sinks.push(sink);
    }

    /// Number of registered child sinks.
    pub fn child_count(&self) -> usize {
        self.sinks.len()
    }
}

impl OutputSink for TeeSink {
    fn write_text(&mut self, text: &str) -> io::Result<()> {
        for sink in &mut self.sinks {
            sink.write_text(text)?;
        }
        Ok(())
    }

    fn write_message(&mut self, msg: &OutputMessage) -> io::Result<()> {
        for sink in &mut self.sinks {
            sink.write_message(msg)?;
        }
        Ok(())
    }

    fn write_chunk(&mut self, chunk: &str) -> io::Result<()> {
        for sink in &mut self.sinks {
            sink.write_chunk(chunk)?;
        }
        Ok(())
    }

    fn write_raw(&mut self, record: &str) -> io::Result<()> {
        for sink in &mut self.sinks {
            sink.write_raw(record)?;
        }
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        for sink in &mut self.sinks {
            sink.flush()?;
        }
        Ok(())
    }
}

/// Shared-sink wrapper: `Arc<Mutex<T>>` participates in fan-out while the
/// caller keeps a handle to read/assert the captured content (and can hand
/// the sink across async task boundaries). Useful for tests and for
/// multi-task writers.
impl<T: OutputSink> OutputSink for Arc<Mutex<T>> {
    fn write_text(&mut self, text: &str) -> io::Result<()> {
        let mut inner = self
            .lock()
            .map_err(|_| io::Error::other("output sink mutex poisoned"))?;
        inner.write_text(text)
    }

    fn write_message(&mut self, msg: &OutputMessage) -> io::Result<()> {
        let mut inner = self
            .lock()
            .map_err(|_| io::Error::other("output sink mutex poisoned"))?;
        inner.write_message(msg)
    }

    fn write_chunk(&mut self, chunk: &str) -> io::Result<()> {
        let mut inner = self
            .lock()
            .map_err(|_| io::Error::other("output sink mutex poisoned"))?;
        inner.write_chunk(chunk)
    }

    fn write_raw(&mut self, record: &str) -> io::Result<()> {
        let mut inner = self
            .lock()
            .map_err(|_| io::Error::other("output sink mutex poisoned"))?;
        inner.write_raw(record)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut inner = self
            .lock()
            .map_err(|_| io::Error::other("output sink mutex poisoned"))?;
        inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_sink_records_all_call_kinds() {
        let mut sink = MemorySink::new();
        sink.write_text("hello").unwrap();
        sink.write_message(&OutputMessage::new("assistant", "hi"))
            .unwrap();
        sink.write_chunk("wor").unwrap();
        sink.flush().unwrap();

        assert_eq!(sink.text(), "hellowor");
        assert_eq!(sink.messages().len(), 1);
        assert_eq!(sink.messages()[0].role, "assistant");
        assert_eq!(sink.flush_count(), 1);
    }

    #[test]
    fn raw_records_bypass_the_format_filter() {
        let mut sink = MemorySink::new();
        sink.write_raw("[ok] execution: done").unwrap();
        sink.write_text("ignored in json").unwrap();
        assert_eq!(sink.raw(), vec!["[ok] execution: done"]);
        assert_eq!(sink.text(), "ignored in json");
    }

    #[test]
    fn text_sink_renders_message_lines() {
        let mut sink = HeadlessFileSink::buffer(Vec::new(), OutputFormat::Text, false);
        sink.write_message(&OutputMessage::new("user", "add test"))
            .unwrap();
        sink.write_text("done").unwrap();
        sink.flush().unwrap();

        let out = String::from_utf8(sink.into_bytes()).unwrap();
        assert_eq!(out, "user: add test\ndone\n");
    }

    #[test]
    fn text_sink_with_color_marks_roles() {
        let mut sink = HeadlessFileSink::buffer(Vec::new(), OutputFormat::Text, true);
        sink.write_message(&OutputMessage::new("assistant", "ok"))
            .unwrap();
        sink.flush().unwrap();

        let out = String::from_utf8(sink.into_bytes()).unwrap();
        assert!(out.contains(BOLD_CYAN));
        assert!(out.contains(RESET));
        // The role is color-marked, so "assistant" and ": ok" are separated by
        // the reset sequence.
        assert!(out.contains("assistant"));
        assert!(out.contains(": ok"));
    }

    #[test]
    fn json_sink_writes_one_object_per_message() {
        let mut sink = HeadlessFileSink::buffer(Vec::new(), OutputFormat::Json, true);
        let msg = OutputMessage::with_meta(
            "assistant",
            "hi",
            serde_json::json!({ "tokens": 12 })
                .as_object()
                .unwrap()
                .clone(),
        );
        sink.write_message(&msg).unwrap();
        sink.flush().unwrap();

        let out = String::from_utf8(sink.into_bytes()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
        assert_eq!(parsed["role"], "assistant");
        assert_eq!(parsed["content"], "hi");
        assert_eq!(parsed["meta"]["tokens"], 12);
    }

    #[test]
    fn chunk_writes_are_text_only() {
        let mut sink = HeadlessFileSink::buffer(Vec::new(), OutputFormat::Text, false);
        sink.write_chunk("par").unwrap();
        sink.write_chunk("tial").unwrap();
        let out = String::from_utf8(sink.into_bytes()).unwrap();
        assert_eq!(out, "partial");
    }

    #[test]
    fn structured_sinks_ignore_chunks_and_free_text() {
        let mut sink = HeadlessFileSink::buffer(Vec::new(), OutputFormat::JsonLines, true);
        sink.write_chunk("partial delta").unwrap();
        sink.write_text("free text").unwrap();
        sink.write_message(&OutputMessage::new("user", "q"))
            .unwrap();
        sink.flush().unwrap();

        let out = String::from_utf8(sink.into_bytes()).unwrap();
        assert!(out.starts_with("{\"role\":\"user\""));
        assert!(!out.contains("partial delta"));
        assert!(!out.contains("free text"));
    }

    #[test]
    fn silent_sink_drops_everything() {
        let mut sink = HeadlessFileSink::buffer(Vec::new(), OutputFormat::Silent, true);
        sink.write_text("x").unwrap();
        sink.write_message(&OutputMessage::new("a", "b")).unwrap();
        sink.write_chunk("c").unwrap();
        sink.flush().unwrap();
        assert!(sink.into_bytes().is_empty());
    }

    #[test]
    fn tee_sink_fans_out_to_all_children() {
        let shared_a = Arc::new(Mutex::new(MemorySink::new()));
        let shared_b = Arc::new(Mutex::new(MemorySink::new()));
        let mut tee = TeeSink::new(vec![Box::new(shared_a.clone()), Box::new(shared_b.clone())]);
        tee.write_text("fan").unwrap();
        tee.write_message(&OutputMessage::new("assistant", "out"))
            .unwrap();
        tee.write_chunk("!").unwrap();
        tee.flush().unwrap();

        assert_eq!(tee.child_count(), 2);
        assert_eq!(shared_a.lock().unwrap().text(), "fan!");
        assert_eq!(shared_b.lock().unwrap().text(), "fan!");
        assert_eq!(shared_a.lock().unwrap().messages().len(), 1);
        assert_eq!(shared_a.lock().unwrap().flush_count(), 1);
    }

    #[test]
    fn envelope_renders_per_format() {
        let ok = OutputEnvelope::success("execution", serde_json::json!({ "executionId": "e1" }));
        assert!(ok.render(OutputFormat::Text).unwrap().contains("[ok]"));
        assert!(ok
            .render(OutputFormat::Json)
            .unwrap()
            .contains("\"success\":true"));
        assert!(ok.render(OutputFormat::JsonLines).is_none());
        assert!(ok.render(OutputFormat::Silent).is_none());

        let fail = OutputEnvelope::failure("execution", "boom");
        let parsed: serde_json::Value =
            serde_json::from_str(fail.render(OutputFormat::Json).unwrap().trim()).unwrap();
        assert_eq!(parsed["success"], false);
        assert_eq!(parsed["message"], "boom");
        assert_eq!(parsed["type"], "execution");
    }
}
