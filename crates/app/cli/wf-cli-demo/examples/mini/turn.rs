//! Mini turn demo: one scripted turn split into stdout and stderr.
//!
//! Replays markdown deltas, one tool, reasoning, usage, and a sub-agent,
//! then shows the same script with benign streams hidden (failures still
//! show). Display knobs come from the real `wf-mini` `DisplayConfig` so the
//! demo cannot drift from production settings. `out|` lines are stdout;
//! `err|` lines are stderr.
//!
//! Run with:
//! ```sh
//! cargo run -p wf-cli-demo --example mini_turn
//! cargo run -p wf-cli-demo --example mini_turn -- --emit
//! ```

use wf_cli_demo::{
    emit,
    mini_turn::{annotated, demo_script, run_turn},
};
use wf_mini::display::DisplayConfig;

fn main() {
    if std::env::args().any(|arg| arg == "--emit") {
        emit::emit(
            "mini",
            "turn_all_on",
            &annotated(&run_turn(DisplayConfig::default(), &demo_script())),
        );
        emit::emit(
            "mini",
            "turn_tools_off",
            &annotated(&run_turn(hidden_streams(), &demo_script())),
        );
        return;
    }
    println!("mini turn: {}", DisplayConfig::default().describe());
    println!("--- all streams on ---");
    print!(
        "{}",
        annotated(&run_turn(DisplayConfig::default(), &demo_script()))
    );
    println!("--- benign streams hidden ---");
    print!("{}", annotated(&run_turn(hidden_streams(), &demo_script())));
}

/// Default display with every benign stderr stream hidden.
fn hidden_streams() -> DisplayConfig {
    DisplayConfig {
        show_tools: false,
        show_reasoning: false,
        show_usage: false,
        ..DisplayConfig::default()
    }
}
