//! Scenario recording and replay for TUI interaction runs.
//!
//! A scenario is a *portable* interaction transcript: metadata, an
//! ordered [`RecordedEvent`] sequence, and enough clock values to replay it
//! deterministically against a headless backend. It is intentionally a data
//! format only; the recorder never owns a terminal or a real clock.

use serde::{Deserialize, Serialize};

use tui_core::render_model::{EventPlayer, EventRecorder, RecordedEvent};

/// Portable transcript of one TUI interaction run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TuiScenario {
    /// Human-readable label for the scenario.
    pub name: String,
    /// Initial terminal width in columns.
    pub width: u16,
    /// Initial terminal height in rows.
    pub height: u16,
    /// Logical events in arrival order.
    pub events: Vec<RecordedEvent>,
}

impl Default for TuiScenario {
    fn default() -> Self {
        Self {
            name: String::new(),
            width: 80,
            height: 24,
            events: Vec::new(),
        }
    }
}

impl TuiScenario {
    /// Build a scenario from an existing recorder and terminal size.
    pub fn from_recorder(name: impl Into<String>, width: u16, height: u16, recorder: &EventRecorder) -> Self {
        Self {
            name: name.into(),
            width: width.max(1),
            height: height.max(1),
            events: recorder.events().to_vec(),
        }
    }

    /// Parse a scenario from JSON. Malformed input yields an empty scenario
    /// instead of panicking so a bad artifact cannot break tests.
    pub fn from_json(raw: &str) -> Self {
        serde_json::from_str(raw).unwrap_or_default()
    }

    /// Serialize the scenario to pretty JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// Number of recorded events.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Whether the scenario carries no events.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Replay cursor positioned at the first event.
    pub fn player(&self) -> EventPlayer {
        let json = serde_json::to_string(&self.events).unwrap_or_else(|_| "[]".to_string());
        EventPlayer::from_json(&json)
    }
}

/// In-memory recorder that stamps events with an explicit clock. The caller
/// supplies clock values so tests stay deterministic; no wall-clock read is
/// performed here.
#[derive(Debug, Default)]
pub struct ScenarioRecorder {
    width: u16,
    height: u16,
    name: String,
    recorder: EventRecorder,
}

impl ScenarioRecorder {
    /// New recorder for a terminal of the given size.
    pub fn new(name: impl Into<String>, width: u16, height: u16) -> Self {
        Self {
            width: width.max(1),
            height: height.max(1),
            name: name.into(),
            recorder: EventRecorder::new(),
        }
    }

    /// Record a key press at `at_ms`.
    pub fn key(&mut self, label: impl Into<String>, at_ms: u64) {
        self.recorder.push(RecordedEvent::Key {
            label: label.into(),
            at_ms,
        });
    }

    /// Record a resize at `at_ms`.
    pub fn resize(&mut self, width: u16, height: u16, at_ms: u64) {
        self.width = width.max(1);
        self.height = height.max(1);
        self.recorder.push(RecordedEvent::Resize {
            width: self.width,
            height: self.height,
            at_ms,
        });
    }

    /// Record a streamed text delta at `at_ms`.
    pub fn delta(&mut self, text: impl Into<String>, at_ms: u64) {
        self.recorder.push(RecordedEvent::Delta {
            text: text.into(),
            at_ms,
        });
    }

    /// Record a frame tick at `at_ms`.
    pub fn tick(&mut self, at_ms: u64) {
        self.recorder.push(RecordedEvent::Tick(at_ms));
    }

    /// Recorded scenario snapshot.
    pub fn scenario(&self) -> TuiScenario {
        TuiScenario::from_recorder(&self.name, self.width, self.height, &self.recorder)
    }

    /// Serialized form of the recorded scenario.
    pub fn to_json(&self) -> String {
        self.scenario().to_json()
    }

    /// Whether nothing has been recorded.
    pub fn is_empty(&self) -> bool {
        self.recorder.events().is_empty()
    }
}

/// Parse a scenario and hand back its event player for headless replay.
pub fn replay_scenario_json(raw: &str) -> EventPlayer {
    let scenario = TuiScenario::from_json(raw);
    let json = serde_json::to_string(&scenario.events).unwrap_or_else(|_| "[]".to_string());
    EventPlayer::from_json(&json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recorder_round_trips_json() {
        let mut recorder = ScenarioRecorder::new("roundtrip", 80, 24);
        recorder.key("enter", 0);
        recorder.resize(100, 30, 10);
        recorder.delta("hello", 20);
        recorder.tick(30);

        let json = recorder.to_json();
        let scenario = TuiScenario::from_json(&json);
        assert_eq!(scenario.name, "roundtrip");
        assert_eq!(scenario.width, 100);
        assert_eq!(scenario.height, 30);
        assert_eq!(scenario.len(), 4);

        let mut player = scenario.player();
        assert!(player.next_event().is_some());
        assert_eq!(player.next_event().and_then(|e| e.at_ms()), Some(10));
    }

    #[test]
    fn malformed_scenario_is_empty() {
        let scenario = TuiScenario::from_json("{not json");
        assert!(scenario.is_empty());
        assert!(replay_scenario_json("{bad").exhausted());
    }
}