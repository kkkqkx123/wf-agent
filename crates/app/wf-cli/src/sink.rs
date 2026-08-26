//! Mini-session output sink.
//!
//! During a mini session the terminal bottom area (`Viewport::Inline`) is
//! owned by the renderer, so business output must never go straight to
//! stdout. [`MiniSink`] forwards every write into an in-memory channel that
//! the renderer drains into the scrollback; `flush` emits a repaint request.
//!
//! Callers keep writing through the [`OutputSink`] trait — the destination
//! decision stays in the CLI layer. An optional `--log <file>` tees the same
//! writes into a clean file via [`TeeSink`] (see [`MiniSink::tee_log`]).

use std::io;
use std::path::Path;

use tokio::sync::mpsc::UnboundedSender;

use crate::error::CliResult;
use crate::output::{HeadlessFileSink, OutputFormat, OutputMessage, OutputSink};
use crate::scrollback::Role;

/// Events emitted by the mini sink for the renderer to consume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MiniOutputEvent {
    /// Free text with the display role to use in the scrollback.
    Text { role: Role, content: String },
    /// A structured conversation message.
    Message(OutputMessage),
    /// An incremental text delta (streaming assistant output).
    Chunk(String),
    /// Repaint request (the sink `flush` boundary).
    Flush,
}

/// Output sink forwarding every write into an unbounded channel. The mini
/// renderer owns the receiver side and repaints from the drained events.
#[derive(Debug, Clone)]
pub struct MiniSink {
    tx: UnboundedSender<MiniOutputEvent>,
}

impl MiniSink {
    /// Sink bound to the given channel; the caller keeps the receiver.
    pub fn new(tx: UnboundedSender<MiniOutputEvent>) -> Self {
        Self { tx }
    }

    /// Write free text with an explicit scrollback role.
    pub fn write_role(&mut self, role: Role, text: &str) -> io::Result<()> {
        self.send_event(MiniOutputEvent::Text {
            role,
            content: text.to_string(),
        })
    }

    /// Build the pair behind `--log <file>`: the in-memory sink plus a clean
    /// file sink receiving the same writes. The caller fans out with
    /// [`TeeSink`] and drops the file sink when the session ends.
    pub fn tee_log(
        tx: UnboundedSender<MiniOutputEvent>,
        path: &Path,
        format: OutputFormat,
    ) -> CliResult<(Self, HeadlessFileSink)> {
        // Files never receive ANSI escapes.
        let file = HeadlessFileSink::file(path, format, false)?;
        Ok((Self::new(tx), file))
    }

    fn send_event(&mut self, event: MiniOutputEvent) -> io::Result<()> {
        self.tx
            .send(event)
            .map_err(|_| io::Error::other("mini sink receiver dropped"))
    }
}

impl OutputSink for MiniSink {
    fn write_text(&mut self, text: &str) -> io::Result<()> {
        self.send_event(MiniOutputEvent::Text {
            role: Role::Default,
            content: text.to_string(),
        })
    }

    fn write_message(&mut self, msg: &OutputMessage) -> io::Result<()> {
        self.send_event(MiniOutputEvent::Message(msg.clone()))
    }

    fn write_chunk(&mut self, chunk: &str) -> io::Result<()> {
        self.send_event(MiniOutputEvent::Chunk(chunk.to_string()))
    }

    fn write_raw(&mut self, _record: &str) -> io::Result<()> {
        // Pre-rendered records (envelope lines) are a headless-stream
        // concept; the renderer has no slot for them, and a `--log` file
        // sink in the same tee captures them instead.
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.send_event(MiniOutputEvent::Flush)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::OutputMessage;

    fn channel() -> (
        MiniSink,
        tokio::sync::mpsc::UnboundedReceiver<MiniOutputEvent>,
    ) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (MiniSink::new(tx), rx)
    }

    #[test]
    fn encodes_all_call_kinds() {
        let (mut sink, mut rx) = channel();
        sink.write_text("hello").unwrap();
        sink.write_message(&OutputMessage::new("assistant", "hi"))
            .unwrap();
        sink.write_chunk("wor").unwrap();
        sink.flush().unwrap();

        assert_eq!(
            rx.try_recv().unwrap(),
            MiniOutputEvent::Text {
                role: Role::Default,
                content: "hello".to_string(),
            }
        );
        assert_eq!(
            rx.try_recv().unwrap(),
            MiniOutputEvent::Message(OutputMessage::new("assistant", "hi"))
        );
        assert_eq!(
            rx.try_recv().unwrap(),
            MiniOutputEvent::Chunk("wor".to_string())
        );
        assert_eq!(rx.try_recv().unwrap(), MiniOutputEvent::Flush);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn write_role_sets_the_scrollback_role() {
        let (mut sink, mut rx) = channel();
        sink.write_role(Role::Error, "boom").unwrap();
        assert_eq!(
            rx.try_recv().unwrap(),
            MiniOutputEvent::Text {
                role: Role::Error,
                content: "boom".to_string(),
            }
        );
    }

    #[test]
    fn raw_records_are_dropped() {
        let (mut sink, mut rx) = channel();
        sink.write_raw("[ok] execution: done").unwrap();
        sink.write_text("kept").unwrap();
        assert!(matches!(
            rx.try_recv().unwrap(),
            MiniOutputEvent::Text { content, .. } if content == "kept"
        ));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn send_after_receiver_drop_errors() {
        let (mut sink, rx) = channel();
        drop(rx);
        assert!(sink.write_text("lost").is_err());
        assert!(sink.flush().is_err());
    }

    #[test]
    fn tee_log_creates_a_clean_file_sink() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mini.log");
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let (mut mini, mut file) = MiniSink::tee_log(tx, &path, OutputFormat::Text).unwrap();

        mini.write_text("hello").unwrap();
        mini.write_message(&OutputMessage::new("user", "world"))
            .unwrap();
        mini.write_chunk(".").unwrap();
        mini.flush().unwrap();
        file.write_text("hello").unwrap();
        file.write_message(&OutputMessage::new("user", "world"))
            .unwrap();
        file.write_chunk(".").unwrap();
        file.flush().unwrap();

        // The in-memory side keeps the same events.
        assert_eq!(
            rx.try_recv().unwrap(),
            MiniOutputEvent::Text {
                role: Role::Default,
                content: "hello".to_string(),
            }
        );

        // The file side holds the clean text stream.
        let on_disk = std::fs::read_to_string(&path).unwrap();
        assert_eq!(on_disk, "hello\nuser: world\n.");
    }
}
