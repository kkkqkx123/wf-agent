//! Mini input demo: slash commands, history, ghost suggestion, approval.
//!
//! Shows how typed lines classify (command or prompt), how history backs
//! `/history` and `/rerun`, what ghost text the prompt draws, and which
//! approval answers allow a tool.
//!
//! Run with:
//! ```sh
//! cargo run -p wf-cli-demo --example mini_input
//! cargo run -p wf-cli-demo --example mini_input -- --emit
//! ```

use unicode_width::UnicodeWidthStr;
use wf_cli_demo::{
    emit,
    mini_input::{
        approval_prompt, is_affirmative, parse_command, partial_word_bytes, seeded_history,
    },
};
use wf_cli_shared::display::{parse_toggle, MarkdownMode};

fn slash_section() -> String {
    let inputs = [
        "/help",
        "/history cargo",
        "/history a b",
        "/rerun",
        "/rerun 2",
        "/rerun 0",
        "/markdown plain",
        "/reasoning off",
        "/tools maybe",
        "/display",
        "/unknown",
        "plain prompt",
    ];
    let mut out = String::new();
    for input in inputs {
        out.push_str(&format!("{input:?} -> {:?}\n", parse_command(input)));
    }
    out.push_str(&format!("toggle off parses: {:?}\n", parse_toggle("off")));
    out.push_str(&format!(
        "markdown plain parses: {:?}\n",
        MarkdownMode::parse_opt("plain")
    ));
    out
}

fn history_section() -> String {
    let mut history = seeded_history();
    history.push("cargo nextest run");
    history.push("cargo nextest run");
    let mut out = String::new();
    out.push_str(&format!("recent: {:?}\n", history.recent(5)));
    out.push_str(&format!(
        "suggest cargo prefix: {:?}\n",
        history.suggest("cargo ")
    ));
    out.push_str(&format!(
        "filter cargo: {:?}\n",
        history.filter_prefix("cargo")
    ));
    out.push_str(&format!("rerun 2: {:?}\n", history.get(2)));
    out.push_str(&format!("rerun 9: {:?}\n", history.get(9)));
    let buffer = "cargo ";
    let ghost = history.suggest(buffer).unwrap_or_default();
    let width = UnicodeWidthStr::width("> ")
        + UnicodeWidthStr::width(buffer)
        + UnicodeWidthStr::width(ghost.as_str());
    out.push_str(&format!(
        "prompt view: `> {buffer}` ghost `{ghost}` width {width}\n"
    ));
    out.push_str(&format!("accept full: {:?}\n", format!("{buffer}{ghost}")));
    out.push_str(&format!(
        "partial accept bytes: {}\n",
        partial_word_bytes(&format!(" {ghost}"))
    ));
    out.push_str(
        "browse Up from `cargo`: cargo nextest run, then cargo test, then stop at oldest\n",
    );
    out.push_str(&format!(
        "{}\n",
        approval_prompt("write_file", Some("low"), 120)
    ));
    out.push_str(&format!(
        "approval y/yes allow, others deny: {:?}\n",
        ["y", "yes", "n", ""]
            .iter()
            .map(|a| is_affirmative(a))
            .collect::<Vec<_>>()
    ));
    out
}

fn section() -> String {
    format!("{}{}", slash_section(), history_section())
}

fn main() {
    if std::env::args().any(|arg| arg == "--emit") {
        emit::emit("mini", "slash_history", &section());
        return;
    }
    println!("mini input: `> ` prompt with ghost suggestion on stderr");
    println!("--- slash classification ---");
    print!("{}", slash_section());
    println!("--- history, suggestion, editor ---");
    print!("{}", history_section());
}
