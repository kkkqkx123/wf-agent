//! Dual-pipeline stdin line reader.

use std::collections::VecDeque;
use std::io::{self, IsTerminal, Write};
use std::time::Duration;

use crossterm::event::{Event, EventStream};
use futures::StreamExt;
use tokio::sync::mpsc;

/// Spawn the pipe-pipeline stdin pump: one detached thread reads raw byte
/// chunks and forwards them into the channel. It is the only stdin reader
/// on the pipe path; nothing else in the process may touch fd 0.
///
/// The thread cannot be interrupted while blocked in `read`; it exits on
/// EOF, read error, or when the channel closes, and the OS reaps it at
/// process exit regardless. `blocking_send` is legal here: the thread was
/// not created by tokio.
fn spawn_stdin_pump(tx: mpsc::UnboundedSender<Vec<u8>>) {
    let _ = std::thread::Builder::new()
        .name("stdin-pump".into())
        .spawn(move || {
            use std::io::Read as _;
            let mut stdin = io::stdin().lock();
            let mut buf = [0u8; 1024];
            loop {
                match stdin.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        // Unbounded `send` is a plain sync method: it never
                        // blocks and works from a non-async thread.
                        if tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                }
            }
        });
}

struct PipeReader {
    rx: Option<mpsc::UnboundedReceiver<Vec<u8>>>,
    pending: Vec<u8>,
    finished: bool,
}

struct TtyReader {
    stream: Option<EventStream>,
    pending_lines: VecDeque<String>,
    partial: String,
    finished: bool,
}

enum Inner {
    Pipe(PipeReader),
    Tty(TtyReader),
}

/// Dual-pipeline line assembler: the single consumer of stdin while the raw
/// editor is not running.
///
/// Pipe path holds the byte channel; TTY path holds the crossterm event
/// stream and assembles cooked lines from key events up to Enter. The
/// channel is unbounded so the pump never blocks: backpressure here
/// would stall interrupt handling while buying nothing (bytes are tiny).
pub struct LineReader {
    inner: Inner,
}

impl LineReader {
    /// Choose the process-wide stdin source once. Call once per process;
    /// the session owns it. TTY stdin uses the event stream as the only
    /// reader (no pump thread competes for fd 0); pipes keep the byte pump.
    pub fn for_stdin() -> Self {
        if io::stdin().is_terminal() {
            Self {
                inner: Inner::Tty(TtyReader {
                    stream: Some(EventStream::new()),
                    pending_lines: VecDeque::new(),
                    partial: String::new(),
                    finished: false,
                }),
            }
        } else {
            let (tx, rx) = mpsc::unbounded_channel();
            spawn_stdin_pump(tx);
            Self {
                inner: Inner::Pipe(PipeReader {
                    rx: Some(rx),
                    pending: Vec::new(),
                    finished: false,
                }),
            }
        }
    }

    /// Wrap an existing channel. Tests use this to feed fake input without
    /// touching real stdin.
    #[cfg(test)]
    pub fn from_receiver(rx: mpsc::UnboundedReceiver<Vec<u8>>) -> Self {
        Self {
            inner: Inner::Pipe(PipeReader {
                rx: Some(rx),
                pending: Vec::new(),
                finished: false,
            }),
        }
    }

    /// Wrap a TTY reader without a live event stream. Tests use this for
    /// pipeline bookkeeping without touching real stdin state.
    #[cfg(test)]
    pub fn for_tty_test() -> Self {
        Self {
            inner: Inner::Tty(TtyReader {
                stream: None,
                pending_lines: VecDeque::new(),
                partial: String::new(),
                finished: false,
            }),
        }
    }

    /// True on the TTY event-stream pipeline.
    pub fn is_tty(&self) -> bool {
        matches!(self.inner, Inner::Tty(_))
    }

    /// Hand the event stream to the raw editor for one prompt. While it is
    /// gone, [`Self::next_line`] reports EOF without latching finished.
    pub fn take_event_stream(&mut self) -> Option<EventStream> {
        match &mut self.inner {
            Inner::Tty(tty) => tty.stream.take(),
            Inner::Pipe(_) => None,
        }
    }

    /// Take the event stream back after the editor finished.
    pub fn restore_event_stream(&mut self, stream: EventStream) {
        if let Inner::Tty(tty) = &mut self.inner {
            tty.stream = Some(stream);
        }
    }

    /// Wait for the next complete line. Returns `None` once stdin hit EOF
    /// and every buffered byte was consumed; an EOF with a pending
    /// unterminated line yields that line (shell convention for piped
    /// input without a trailing newline).
    pub async fn next_line(&mut self) -> Option<String> {
        match &mut self.inner {
            Inner::Pipe(pipe) => pipe_next_line(pipe).await,
            Inner::Tty(tty) => tty_next_line(tty).await,
        }
    }

    /// Drop every buffered but unconsumed byte, returning how many complete
    /// lines were dropped. Never blocks. Runs whenever consumption passes
    /// back to the prompt, so a late approval answer or a line typed
    /// mid-stream is never misread as the next prompt.
    pub fn drain(&mut self) -> usize {
        match &mut self.inner {
            Inner::Pipe(pipe) => pipe_drain(pipe),
            Inner::Tty(tty) => tty_drain(tty),
        }
    }

    /// Print the prompt, flush, then take one line from the shared source.
    ///
    /// Returns `None` on EOF (Ctrl-D). In cooked mode line editing, paste
    /// and input methods are provided by the terminal. The prompt goes to
    /// stderr so stdout carries only assistant text and stays clean under
    /// pipe redirection.
    pub async fn read_prompt_line(&mut self, prompt: &str) -> io::Result<Option<String>> {
        {
            let stderr = io::stderr();
            let mut err = stderr.lock();
            err.write_all(prompt.as_bytes())?;
            err.flush()?;
        }
        Ok(self.next_line().await)
    }
}

async fn pipe_next_line(pipe: &mut PipeReader) -> Option<String> {
    loop {
        if let Some(pos) = pipe.pending.iter().position(|&byte| byte == b'\n') {
            let mut line: Vec<u8> = pipe.pending.drain(..=pos).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            return Some(String::from_utf8_lossy(&line).into_owned());
        }
        if pipe.finished {
            return None;
        }
        let Some(rx) = pipe.rx.as_mut() else {
            // Receiver handed to the raw editor: EOF for this consumer,
            // but do not latch `finished` — the receiver comes back
            // when the prompt ends.
            return None;
        };
        match rx.recv().await {
            Some(chunk) => pipe.pending.extend_from_slice(&chunk),
            None => {
                pipe.finished = true;
                if pipe.pending.is_empty() {
                    return None;
                }
                let rest = std::mem::take(&mut pipe.pending);
                return Some(String::from_utf8_lossy(&rest).into_owned());
            }
        }
    }
}

fn pipe_drain(pipe: &mut PipeReader) -> usize {
    let mut dropped = pipe.pending.iter().filter(|&&b| b == b'\n').count();
    pipe.pending.clear();
    loop {
        let try_result = pipe.rx.as_mut().map(|rx| rx.try_recv());
        match try_result {
            Some(Ok(chunk)) => dropped += chunk.iter().filter(|&&b| b == b'\n').count(),
            Some(Err(mpsc::error::TryRecvError::Empty)) | None => return dropped,
            Some(Err(mpsc::error::TryRecvError::Disconnected)) => {
                pipe.finished = true;
                return dropped;
            }
        }
    }
}

/// TTY cooked-line assembly from key events. The terminal stays in cooked
/// mode here so Ctrl-C still arrives as SIGINT (handled by the caller's
/// select); control and alt combinations are ignored for line purposes and
/// only plain characters, Backspace and Enter shape the line.
async fn tty_next_line(tty: &mut TtyReader) -> Option<String> {
    loop {
        if let Some(line) = tty.pending_lines.pop_front() {
            return Some(line);
        }
        if tty.finished {
            if tty.partial.is_empty() {
                return None;
            }
            return Some(std::mem::take(&mut tty.partial));
        }
        let stream = tty.stream.as_mut()?;
        match stream.next().await {
            Some(Ok(Event::Key(key))) => {
                if key.kind.is_release() {
                    continue;
                }
                use crossterm::event::{KeyCode as C, KeyModifiers as M};
                if key.modifiers.contains(M::CONTROL) || key.modifiers.contains(M::ALT) {
                    continue;
                }
                match key.code {
                    C::Enter => {
                        return Some(std::mem::take(&mut tty.partial));
                    }
                    C::Backspace => {
                        tty.partial.pop();
                    }
                    C::Char(c) => {
                        tty.partial.push(c);
                    }
                    _ => {}
                }
            }
            Some(Ok(Event::Mouse(mouse))) => {
                // No mouse capture in mini: buttons stay native and wheel
                // notches have no cooked-line target (history browsing
                // lives in the raw editor path). Named explicitly so a
                // future capture change lands here first.
                use crossterm::event::MouseEventKind;
                match mouse.kind {
                    MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {}
                    _ => {}
                }
            }
            Some(Ok(Event::Paste(_))) | Some(Ok(Event::Resize(_, _))) | Some(Ok(_)) => {}
            Some(Err(_)) | None => {
                tty.finished = true;
                if tty.partial.is_empty() && tty.pending_lines.is_empty() {
                    return None;
                }
                if let Some(line) = tty.pending_lines.pop_front() {
                    return Some(line);
                }
                if !tty.partial.is_empty() {
                    return Some(std::mem::take(&mut tty.partial));
                }
                return None;
            }
        }
    }
}

/// Non-blocking TTY drain: queued lines plus any ready key events. The
/// event stream is idle while draining (no concurrent poll), so a zero
/// timeout poll plus read loop is safe.
fn tty_drain(tty: &mut TtyReader) -> usize {
    let mut dropped = tty.pending_lines.len();
    tty.pending_lines.clear();
    tty.partial.clear();
    if tty.stream.is_none() {
        return dropped;
    }
    let mut partial = String::new();
    while let Ok(true) = crossterm::event::poll(Duration::from_secs(0)) {
        match crossterm::event::read() {
            Ok(Event::Key(key)) => {
                if key.kind.is_release() {
                    continue;
                }
                use crossterm::event::{KeyCode as C, KeyModifiers as M};
                if key.modifiers.contains(M::CONTROL) || key.modifiers.contains(M::ALT) {
                    continue;
                }
                match key.code {
                    C::Enter => {
                        dropped += 1;
                        partial.clear();
                    }
                    C::Backspace => {
                        partial.pop();
                    }
                    C::Char(c) => {
                        partial.push(c);
                    }
                    _ => {}
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    dropped
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_reader() -> (LineReader, mpsc::UnboundedSender<Vec<u8>>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (LineReader::from_receiver(rx), tx)
    }

    #[tokio::test]
    async fn reader_delivers_lines_in_order() {
        let (mut reader, tx) = test_reader();
        tx.send(b"first\n".to_vec()).unwrap();
        tx.send(b"second\n".to_vec()).unwrap();
        assert_eq!(reader.next_line().await.as_deref(), Some("first"));
        assert_eq!(reader.next_line().await.as_deref(), Some("second"));
    }

    #[tokio::test]
    async fn reader_reassembles_lines_split_across_chunks() {
        let (mut reader, tx) = test_reader();
        tx.send(b"hel".to_vec()).unwrap();
        tx.send("lo世\n".as_bytes().to_vec()).unwrap();
        assert_eq!(reader.next_line().await.as_deref(), Some("hello世"));
    }

    #[tokio::test]
    async fn reader_reports_closed_after_sender_drop_and_drain() {
        let (mut reader, tx) = test_reader();
        tx.send(b"late\n".to_vec()).unwrap();
        drop(tx);
        assert_eq!(reader.next_line().await.as_deref(), Some("late"));
        assert_eq!(reader.next_line().await, None);
        assert_eq!(reader.next_line().await, None);
    }

    #[tokio::test]
    async fn drain_drops_stale_lines_and_counts_them() {
        let (mut reader, tx) = test_reader();
        tx.send(b"stale-1\n".to_vec()).unwrap();
        tx.send(b"stale-2\n".to_vec()).unwrap();
        assert_eq!(reader.drain(), 2);
        assert_eq!(reader.drain(), 0);
        tx.send(b"fresh\n".to_vec()).unwrap();
        assert_eq!(reader.next_line().await.as_deref(), Some("fresh"));
    }

    #[tokio::test]
    async fn drain_on_closed_channel_marks_finished() {
        let (mut reader, tx) = test_reader();
        tx.send(b"x\n".to_vec()).unwrap();
        drop(tx);
        assert_eq!(reader.drain(), 1);
        assert_eq!(reader.next_line().await, None);
    }

    #[tokio::test]
    async fn eof_yields_pending_unterminated_line() {
        let (mut reader, tx) = test_reader();
        tx.send(b"no trailing newline".to_vec()).unwrap();
        drop(tx);
        assert_eq!(
            reader.next_line().await.as_deref(),
            Some("no trailing newline")
        );
        assert_eq!(reader.next_line().await, None);
    }

    #[tokio::test]
    async fn stream_borrow_reports_eof_while_borrowed() {
        let mut reader = LineReader::for_tty_test();
        assert!(reader.is_tty());
        assert!(reader.next_line().await.is_none(), "no stream: EOF");
        assert_eq!(reader.drain(), 0);
    }

    #[test]
    fn tty_reader_reports_tty_and_drains_empty() {
        let mut reader = LineReader::for_tty_test();
        assert!(reader.is_tty());
        assert!(reader.take_event_stream().is_none());
        assert_eq!(reader.drain(), 0);
    }

    #[test]
    fn tty_drain_empty_is_zero_and_pipe_is_not_tty() {
        let mut tty = LineReader::for_tty_test();
        assert_eq!(tty.drain(), 0);
        let (tx, rx) = mpsc::unbounded_channel();
        drop(tx);
        let pipe = LineReader::from_receiver(rx);
        assert!(!pipe.is_tty());
    }
}
