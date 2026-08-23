//! Deterministic golden-output generator for the shared UI components.
//!
//! The lib tests stay side-effect free (pure in-memory assertions). This
//! example is the single place that *writes* rendering output, regenerating
//! the reference files in `crates/wf-cli/outputs/` so they are reviewable and
//! diffable without polluting `src/`.
//!
//! Run with:
//! ```sh
//! cargo run -p wf-cli --example component_output
//! ```
//!
//! Output is the plain-text rendering of each component at the documented
//! widths, one file per capture, plus a final newline per file.

use std::fs;
use std::path::PathBuf;

use wf_cli::ansi::{plain_text, AnsiParser};
use wf_cli::scrollback::{lines_to_string, HistoryLine, LineState, Role};
use wf_cli::select::{Group, GroupItem, SelectList};

/// The crate's `outputs/` directory (rooted at the manifest dir, not CWD).
fn outputs_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("outputs")
}

/// Write `content` plus a trailing newline to `outputs/<name>.txt`.
fn emit(name: &str, content: &str) {
    let dir = outputs_dir();
    fs::create_dir_all(&dir).expect("create outputs dir");
    let path = dir.join(format!("{name}.txt"));
    fs::write(&path, format!("{content}\n")).expect("write output file");
    eprintln!("wrote {}", path.display());
}

/// Join the plain text of rendered lines with newlines (mirrors the pure
/// helper used by the lib tests, so example output stays in lock-step).
fn join_lines<'a>(lines: impl Iterator<Item = &'a ratatui::text::Line<'a>>) -> String {
    lines
        .map(|l| {
            l.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn scrollback_captures() {
    let h = HistoryLine::new_with_role(
        "  check inspect it code everywhere seat output",
        LineState::Committed,
        Role::Accent,
    );
    emit("reflow_w12", &lines_to_string(&h.display_lines(12)));
    emit("reflow_w40", &lines_to_string(&h.display_lines(40)));
}

fn ansi_capture() {
    let input = concat!(
        "\x1b[1;32mok\x1b[0m ",
        "\x1b[38;5;208morange\x1b[0m ",
        "\x1b[38;2;100;200;50mtruecol\r\n",
        "tab:\tend"
    );
    emit(
        "ansi_mixed",
        &plain_text(&AnsiParser::new().parse(input.as_bytes())),
    );
}

fn select_captures() {
    let mut list: SelectList<u32> = SelectList::groups(vec![
        Group::new(Some("Workflows"))
            .item(GroupItem::new("wf-a", 1))
            .item(GroupItem::new("wf-b", 2)),
        Group::new(Some("Executions"))
            .item(GroupItem::new("exec-1", 3).described("running"))
            .item(GroupItem::new("exec-2", 4).described("idle"))
            .item(GroupItem::new("exec-3", 5)),
    ]);
    list.move_to(3);
    emit("select_wide", &join_lines(list.render_lines(80, 5).iter()));
    emit(
        "select_narrow_single_col",
        &join_lines(list.render_lines(20, 5).iter()),
    );
}

fn main() {
    scrollback_captures();
    ansi_capture();
    select_captures();
    eprintln!(
        "golden outputs regenerated under {}",
        outputs_dir().display()
    );
}
