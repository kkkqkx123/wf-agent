//! TUI scenario recording and replay demo.
//!
//! Records a small interaction transcript with deterministic clock values,
//! serializes it to `scenario.json`, then replays the recorded events and
//! prints a compact summary. This shows how to integrate the scenario format
//! into a demo or a regression harness without opening a terminal.
//!
//! Run with:
//! ```sh
//! cargo run -p wf-cli-demo --example tui_scenario
//! ```

use std::fs;

use wf_tui::clock;
use wf_tui::tui_debug::{ScenarioRecorder, TuiScenario};

fn main() {
    // Deterministic time keeps the recorded transcript reproducible.
    clock::enable_test_clock(0);

    let mut recorder = ScenarioRecorder::new("demo", 80, 24);
    recorder.resize(100, 30, 1);
    recorder.key("enter", 2);
    recorder.delta("hello", 3);
    recorder.tick(4);
    recorder.key("q", 5);

    let json = recorder.to_json();
    let _ = fs::write("scenario.json", &json);

    let scenario = TuiScenario::from_json(&json);
    let mut player = scenario.player();
    let mut count = 0usize;
    while player.next_event().is_some() {
        count += 1;
    }

    clock::disable_test_clock();

    println!("recorded_events={}", scenario.len());
    println!("scenario_bytes={}", json.len());
    println!("replayed_events={count}");
    println!("scenario_file=scenario.json");
}