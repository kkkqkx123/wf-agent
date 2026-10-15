//! Cross-crate invariants of the headless summary kernel
//! (`tui_core::headless::HeadlessRenderer`).
//!
//! These tests live in `tui-render` because it is the only crate that
//! depends on all three participants of the same-source pipeline
//! (reducer commits, streaming markdown, scrollback rendering).

use tui_components::transcript::{lines_to_string, HistoryLine};
use tui_core::headless::HeadlessRenderer;
use tui_core::reducer::{fold, MiniCommit};
use wf_types::events::ExecutionStreamEvent;

fn delta(text: &str) -> ExecutionStreamEvent {
    ExecutionStreamEvent::LlmDelta {
        content: text.to_string(),
    }
}

/// Same-source test: the same synthetic event sequence drives both
/// HeadlessRenderer and SessionReducer -> MiniCommit -> HistoryLine,
/// asserting text consistency - headless output and TUI inline scrollback
/// share the same reducer output.
#[test]
fn headless_stdout_matches_mini_scrollback_from_same_reducer() {
    let events = vec![
        delta("Hello, "),
        delta("world!\n"),
        delta("Let me inspect the file "),
        delta("and report back."),
        ExecutionStreamEvent::Completed {
            result: serde_json::Value::Null,
            iterations: 1,
        },
    ];

    // ① HeadlessRenderer stdout (streaming).
    let mut renderer = HeadlessRenderer::new("exec-same");
    let mut stdout = String::new();
    for event in &events {
        stdout.push_str(&renderer.on_event(event).stdout);
    }
    stdout.push_str(&renderer.finish().stdout);

    // ② Same reducer → MiniCommit → HistoryLine (the TUI inline scrollback
    //    source text renders from these commits).
    let (commits, _footer) = fold(&events, "exec-same");
    let mut scrollback = String::new();
    for commit in &commits {
        if let MiniCommit::AssistantText { content } = commit {
            let line = HistoryLine::markdown(content.clone());
            if !scrollback.is_empty() {
                scrollback.push('\n');
            }
            scrollback.push_str(&lines_to_string(&line.display_lines(80)));
        }
    }

    let expected = "Hello, world!\nLet me inspect the file and report back.";
    assert_eq!(stdout, expected);
    assert_eq!(stdout, scrollback);
}
