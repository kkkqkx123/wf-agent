use std::sync::{Arc, Mutex};

use bytes::Bytes;
use wasmtime_wasi::p2::{OutputStream, Pollable, StdoutStream, StreamError};

/// Per-stream capture capacity for guest `stdout`/`stderr` in bytes.
/// Writes beyond this capacity trap inside the guest instead of growing
/// host memory without bound.
pub const GUEST_STDIO_CAP_BYTES: usize = 64 * 1024;
/// Upper bound for guest stdio bytes forwarded to the host log per stream
/// per call. The full capture is dropped after logging; the cap only bounds
/// log spam, not what the guest may write (see `GUEST_STDIO_CAP_BYTES`).
pub const GUEST_STDIO_LOG_BYTES_CAP: usize = 4096;
/// Maximum lines forwarded per stream per call; further lines are counted
/// and summarized instead of dropped silently.
pub const GUEST_STDIO_LOG_LINES_CAP: usize = 20;

/// Drainable capture pipe for one guest output stream.
///
/// Clones share the same buffer (required by `StdoutStream::stream`), and
/// the host drains it with `take_contents` after every guest call, so pooled
/// sessions never accumulate output across calls.
#[derive(Debug, Clone)]
pub struct GuestLogPipe {
    buffer: Arc<Mutex<Vec<u8>>>,
    capacity: usize,
}

impl GuestLogPipe {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: Arc::new(Mutex::new(Vec::new())),
            capacity,
        }
    }

    /// Take all captured bytes, leaving the buffer empty.
    pub fn take_contents(&self) -> Vec<u8> {
        let mut buffer = self.buffer.lock().expect("guest stdio buffer poisoned");
        std::mem::take(&mut *buffer)
    }
}

impl StdoutStream for GuestLogPipe {
    fn stream(&self) -> Box<dyn OutputStream> {
        Box::new(self.clone())
    }

    fn isatty(&self) -> bool {
        false
    }
}

#[async_trait::async_trait]
impl OutputStream for GuestLogPipe {
    fn write(&mut self, bytes: Bytes) -> Result<(), StreamError> {
        let mut buffer = self.buffer.lock().expect("guest stdio buffer poisoned");
        if bytes.len() > self.capacity.saturating_sub(buffer.len()) {
            return Err(StreamError::trap("guest stdio capture capacity exceeded"));
        }
        buffer.extend_from_slice(bytes.as_ref());
        Ok(())
    }

    fn flush(&mut self) -> Result<(), StreamError> {
        Ok(())
    }

    fn check_write(&mut self) -> Result<usize, StreamError> {
        let len = self
            .buffer
            .lock()
            .expect("guest stdio buffer poisoned")
            .len();
        if len < self.capacity {
            Ok(self.capacity - len)
        } else {
            Err(StreamError::Closed)
        }
    }
}

#[async_trait::async_trait]
impl Pollable for GuestLogPipe {
    async fn ready(&mut self) {}
}

/// Maximum bytes of a single guest-to-host log message. Longer input is
/// truncated so one chatty guest cannot flood the host log.
pub const HOST_LOG_MESSAGE_CAP_BYTES: usize = 4096;

/// Emit one structured guest log record through the host `log` import.
/// Best-effort by design: the message is truncated to
/// [`HOST_LOG_MESSAGE_CAP_BYTES`] and invalid UTF-8 is lossily converted,
/// so the observability path never fails a guest call.
pub fn emit_host_log(plugin_id: &str, level: u32, message: &[u8]) {
    let output = cap_log_message(&String::from_utf8_lossy(message));
    match level {
        0 => tracing::trace!(plugin_id, output, "wasm guest log"),
        1 => tracing::debug!(plugin_id, output, "wasm guest log"),
        2 => tracing::info!(plugin_id, output, "wasm guest log"),
        3 => tracing::warn!(plugin_id, output, "wasm guest log"),
        _ => tracing::error!(plugin_id, output, "wasm guest log"),
    }
}

/// Truncate a log message to [`HOST_LOG_MESSAGE_CAP_BYTES`] on a UTF-8
/// boundary, marking truncation explicitly.
fn cap_log_message(text: &str) -> String {
    if text.len() <= HOST_LOG_MESSAGE_CAP_BYTES {
        return text.to_owned();
    }
    let mut end = HOST_LOG_MESSAGE_CAP_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...(truncated)", &text[..end])
}

/// Drain both pipes and forward any output to the host log. Always drains,
/// even when there is nothing to log, so pooled sessions start the next
/// call with empty buffers. Returns the drained bytes for testability.
pub fn drain_guest_stdio(
    plugin_id: &str,
    op: &str,
    stdout: &GuestLogPipe,
    stderr: &GuestLogPipe,
) -> (Vec<u8>, Vec<u8>) {
    let out = stdout.take_contents();
    let err = stderr.take_contents();
    log_stream(plugin_id, op, "stdout", &out);
    log_stream(plugin_id, op, "stderr", &err);
    (out, err)
}

fn log_stream(plugin_id: &str, op: &str, stream: &str, bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    let text = String::from_utf8_lossy(bytes);
    let mut view = text.as_ref();
    let mut truncated_bytes = 0usize;
    if view.len() > GUEST_STDIO_LOG_BYTES_CAP {
        let mut end = GUEST_STDIO_LOG_BYTES_CAP;
        while !view.is_char_boundary(end) {
            end -= 1;
        }
        truncated_bytes = view.len() - end;
        view = &view[..end];
    }
    let lines: Vec<&str> = view.lines().collect();
    let rendered = if lines.len() > GUEST_STDIO_LOG_LINES_CAP {
        let omitted = lines.len() - GUEST_STDIO_LOG_LINES_CAP;
        format!(
            "{}\n...({omitted} more line(s), {truncated_bytes} more byte(s) truncated)",
            lines[..GUEST_STDIO_LOG_LINES_CAP].join("\n")
        )
    } else if truncated_bytes > 0 {
        format!("{view}\n...({truncated_bytes} more byte(s) truncated)")
    } else {
        view.to_owned()
    };
    if stream == "stderr" {
        tracing::warn!(
            plugin_id,
            op,
            output = %rendered,
            "wasm guest stderr"
        );
    } else {
        tracing::info!(
            plugin_id,
            op,
            output = %rendered,
            "wasm guest stdout"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipe_buffers_and_drains() {
        let pipe = GuestLogPipe::new(16);
        let mut stream = pipe.stream();
        stream
            .write(Bytes::from_static(b"hello"))
            .expect("write fits");
        assert_eq!(pipe.take_contents(), b"hello");
        assert!(pipe.take_contents().is_empty());
    }

    #[test]
    fn pipe_rejects_writes_beyond_capacity() {
        let pipe = GuestLogPipe::new(4);
        let mut stream = pipe.stream();
        assert!(stream.write(Bytes::from_static(b"12345")).is_err());
        stream
            .write(Bytes::from_static(b"1234"))
            .expect("exact fit");
        assert!(stream.write(Bytes::from_static(b"x")).is_err());
    }

    #[test]
    fn clones_share_one_buffer() {
        let pipe = GuestLogPipe::new(64);
        let mut a = pipe.stream();
        let mut b = pipe.stream();
        a.write(Bytes::from_static(b"ab")).expect("write a");
        b.write(Bytes::from_static(b"cd")).expect("write b");
        assert_eq!(pipe.take_contents(), b"abcd");
    }

    #[test]
    fn drain_returns_bytes_and_clears() {
        let stdout = GuestLogPipe::new(64);
        let stderr = GuestLogPipe::new(64);
        stdout
            .stream()
            .write(Bytes::from_static(b"out"))
            .expect("write");
        let (out, err) = drain_guest_stdio("p", "op", &stdout, &stderr);
        assert_eq!(out, b"out");
        assert!(err.is_empty());
        assert!(stdout.take_contents().is_empty());
    }

    #[test]
    fn host_log_message_passes_short_text_through() {
        assert_eq!(cap_log_message("hello"), "hello");
        assert_eq!(cap_log_message(""), "");
    }

    #[test]
    fn host_log_message_truncates_on_char_boundary() {
        let long = "é".repeat(HOST_LOG_MESSAGE_CAP_BYTES);
        let capped = cap_log_message(&long);
        assert!(capped.ends_with("...(truncated)"), "got tail");
        let body = capped.strip_suffix("...(truncated)").expect("suffix");
        assert!(body.len() <= HOST_LOG_MESSAGE_CAP_BYTES);
        assert!(body.is_char_boundary(body.len()));
    }
}
