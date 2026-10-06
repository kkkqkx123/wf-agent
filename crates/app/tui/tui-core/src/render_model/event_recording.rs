//! Interaction event recording and replay for session replay.

use serde::{Deserialize, Serialize};

/// Recorded interaction event for session replay with timestamps and bus
/// coverage. The version field keeps serialized sequences comparable across
/// recorder changes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RecordedEvent {
    /// Logical key press identified by its debug label at `at_ms`.
    Key { label: String, at_ms: u64 },
    /// Terminal resize to columns × rows at `at_ms`.
    Resize { width: u16, height: u16, at_ms: u64 },
    /// Streamed text delta arrival at `at_ms`.
    Delta { text: String, at_ms: u64 },
    /// Background bus notification at `at_ms`.
    Bus { topic: String, at_ms: u64 },
    /// Frame tick at the given clock value.
    Tick(u64),
}

impl RecordedEvent {
    /// Clock value carried by the event, if any.
    pub fn at_ms(&self) -> Option<u64> {
        match self {
            RecordedEvent::Key { at_ms, .. }
            | RecordedEvent::Resize { at_ms, .. }
            | RecordedEvent::Delta { at_ms, .. }
            | RecordedEvent::Bus { at_ms, .. } => Some(*at_ms),
            RecordedEvent::Tick(value) => Some(*value),
        }
    }
}

/// Backwards-compatible constructors for the logical event shapes.
impl RecordedEvent {
    pub fn key(label: impl Into<String>) -> Self {
        Self::Key {
            label: label.into(),
            at_ms: 0,
        }
    }

    pub fn resize(width: u16, height: u16) -> Self {
        Self::Resize {
            width,
            height,
            at_ms: 0,
        }
    }

    pub fn delta(text: impl Into<String>) -> Self {
        Self::Delta {
            text: text.into(),
            at_ms: 0,
        }
    }
}

/// Collects a JSON-serializable interaction sequence.
#[derive(Debug, Default)]
pub struct EventRecorder {
    events: Vec<RecordedEvent>,
}

impl EventRecorder {
    /// Empty recorder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append an event.
    pub fn push(&mut self, event: RecordedEvent) {
        self.events.push(event);
    }

    /// Recorded events in order.
    pub fn events(&self) -> &[RecordedEvent] {
        &self.events
    }

    /// Serialize the sequence to JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.events).unwrap_or_else(|_| "[]".to_string())
    }
}

/// Replays a recorded sequence frame by frame.
#[derive(Debug, Default)]
pub struct EventPlayer {
    events: Vec<RecordedEvent>,
    cursor: usize,
}

impl EventPlayer {
    /// Build a player from a recorded JSON sequence; malformed input yields
    /// an empty player rather than failing.
    pub fn from_json(raw: &str) -> Self {
        let events: Vec<RecordedEvent> = serde_json::from_str(raw).unwrap_or_default();
        Self { events, cursor: 0 }
    }

    /// Next event, if any.
    pub fn next_event(&mut self) -> Option<&RecordedEvent> {
        let event = self.events.get(self.cursor)?;
        self.cursor += 1;
        Some(event)
    }

    /// Whether the whole sequence was consumed.
    pub fn exhausted(&self) -> bool {
        self.cursor >= self.events.len()
    }
}
