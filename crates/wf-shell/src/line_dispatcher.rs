//! Output-line event dispatch.
//!
//! Splits the raw output stream into lines and forwards complete (non-empty)
//! lines to the session [`EventDispatcher`], keeping partial lines across
//! chunks. Dispatch is a non-blocking queue send, so a slow sink never
//! backpressures the reader thread this runs on.

use std::sync::{Arc, Mutex};

use crate::event_sink::{EventDispatcher, ShellEvent};

/// Splits the raw output stream into lines and forwards complete (non-empty)
/// lines to the session [`EventDispatcher`], keeping partial lines across
/// chunks. Empty lines are skipped (aligned with the TS terminal service).
/// Dispatch is a non-blocking queue send, so a slow sink never backpressures
/// the reader thread this runs on.
#[derive(Clone)]
pub(crate) struct OutputLineDispatcher {
    dispatcher: Option<Arc<EventDispatcher>>,
    session_id: String,
    task_id: Arc<Mutex<Option<String>>>,
    pending: String,
}

impl OutputLineDispatcher {
    pub(crate) fn new(
        dispatcher: Option<Arc<EventDispatcher>>,
        session_id: String,
        task_id: Arc<Mutex<Option<String>>>,
    ) -> Self {
        Self {
            dispatcher,
            session_id,
            task_id,
            pending: String::new(),
        }
    }

    pub(crate) fn consume(&mut self, chunk: &str) {
        self.pending.push_str(chunk);
        while let Some(pos) = self.pending.find('\n') {
            let line = self.pending[..pos].to_string();
            self.pending.drain(..=pos);
            self.dispatch(line);
        }
    }

    /// Dispatch any trailing partial line (reached on EOF).
    pub(crate) fn flush(&mut self) {
        if !self.pending.is_empty() {
            let line = std::mem::take(&mut self.pending);
            self.dispatch(line);
        }
    }

    fn dispatch(&self, line: String) {
        let Some(dispatcher) = self.dispatcher.as_ref() else {
            return;
        };
        let trimmed = line.trim_end_matches('\r');
        if trimmed.is_empty() {
            return;
        }
        let task_id = self.task_id.lock().unwrap().clone();
        dispatcher.send(ShellEvent::Output {
            session_id: self.session_id.clone(),
            task_id,
            line: trimmed.to_string(),
        });
    }
}
