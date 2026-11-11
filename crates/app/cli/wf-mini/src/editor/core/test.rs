use super::*;
use crate::suggest::SuggestEngine;

fn core(history: &[&str], prev: Option<&str>) -> EditorCore {
    EditorCore::new(
        EditorSnapshot {
            history: history.iter().map(|s| s.to_string()).collect(),
            prev_prompt: prev.map(|s| s.to_string()),
            prompt: "> ".to_string(),
            term_cols: 80,
            skills: Vec::new(),
            workflows: Vec::new(),
            project_root: None,
            suggestion_display_limit: 30,
        },
        Arc::new(SuggestEngine::new()),
        Arc::new(SuggestBroker::new()),
    )
}

fn feed(core: &mut EditorCore, keys: &[Key]) {
    for key in keys {
        core.handle_key(key.clone());
    }
}

fn type_str(core: &mut EditorCore, text: &str) {
    feed(core, &text.chars().map(Key::Char).collect::<Vec<_>>());
}

fn buffer_str(core: &EditorCore) -> String {
    core.line()
}

#[test]
fn mention_hint_appears_only_over_the_cap() {
    let mut nine = core(&[], None);
    let over: String = (0..9)
        .map(|i| format!("@skill:s{i}"))
        .collect::<Vec<_>>()
        .join(" ");
    type_str(&mut nine, &over);
    assert!(nine.mention_hint().is_some());
    let mut eight = core(&[], None);
    let fits: String = (0..8)
        .map(|i| format!("@skill:s{i}"))
        .collect::<Vec<_>>()
        .join(" ");
    type_str(&mut eight, &fits);
    assert_eq!(eight.mention_hint(), None);
}

#[test]
fn typing_refetches_suggestion_from_history() {
    let mut core = core(&["cargo nextest run"], None);
    type_str(&mut core, "cargo ");
    assert_eq!(core.suggestion_suffix(), Some("nextest run"));
    type_str(&mut core, "n");
    assert_eq!(core.suggestion_suffix(), Some("extest run"));
}

#[test]
fn enter_submits_buffer_without_suggestion() {
    let mut core = core(&["cargo nextest run"], None);
    type_str(&mut core, "cargo ");
    assert_eq!(core.handle_key(Key::Enter), CoreAction::Submit);
    assert_eq!(buffer_str(&core), "cargo ");
}

#[test]
fn cursor_movement_clears_suggestion() {
    let mut core = core(&["cargo nextest run"], None);
    type_str(&mut core, "cargo ");
    assert!(core.suggestion_suffix().is_some());
    assert_eq!(core.handle_key(Key::Left(false)), CoreAction::Continue);
    assert_eq!(core.suggestion_suffix(), None, "clear on cursor move");
}

#[test]
fn right_at_end_accepts_full_suggestion() {
    let mut core = core(&["cargo nextest run"], None);
    type_str(&mut core, "cargo ");
    assert_eq!(core.handle_key(Key::Right(false)), CoreAction::Continue);
    assert_eq!(buffer_str(&core), "cargo nextest run");
    assert_eq!(core.suggestion_suffix(), None);
}

#[test]
fn tab_accepts_full_suggestion() {
    let mut core = core(&["cargo nextest run"], None);
    type_str(&mut core, "cargo");
    assert_eq!(core.handle_key(Key::Tab), CoreAction::Continue);
    assert_eq!(buffer_str(&core), "cargo nextest run");
}

#[test]
fn ctrl_right_accepts_one_word_at_a_time() {
    let mut core = core(&["cargo nextest run"], None);
    type_str(&mut core, "cargo ");
    // "nextest " accepted first…
    assert_eq!(core.handle_key(Key::Right(true)), CoreAction::Continue);
    assert_eq!(buffer_str(&core), "cargo nextest ");
    assert_eq!(core.suggestion_suffix(), Some("run"));
    // …then "run".
    assert_eq!(core.handle_key(Key::AltF), CoreAction::Continue);
    assert_eq!(buffer_str(&core), "cargo nextest run");
    assert_eq!(core.suggestion_suffix(), None);
}

#[test]
fn partial_word_accepts_leading_whitespace_run() {
    assert_eq!(partial_word_bytes(" nextest run"), " ".len());
    assert_eq!(partial_word_bytes("nextest run"), "nextest ".len());
    assert_eq!(partial_word_bytes("nextest"), "nextest".len());
    assert_eq!(partial_word_bytes(""), 0);
}

#[test]
fn up_browses_prefix_matches_newest_first() {
    let mut core = core(&["cargo test", "git status", "cargo nextest run"], None);
    type_str(&mut core, "cargo");
    assert_eq!(core.handle_key(Key::Up), CoreAction::Continue);
    assert_eq!(buffer_str(&core), "cargo nextest run");
    assert_eq!(core.handle_key(Key::Up), CoreAction::Continue);
    assert_eq!(buffer_str(&core), "cargo test");
    assert_eq!(core.handle_key(Key::Up), CoreAction::Continue);
    assert_eq!(buffer_str(&core), "cargo test", "oldest stop");
}

#[test]
fn down_restores_typed_prefix_after_newest() {
    let mut core = core(&["cargo test", "cargo nextest run"], None);
    type_str(&mut core, "cargo");
    feed(&mut core, &[Key::Up, Key::Down, Key::Down]);
    assert_eq!(buffer_str(&core), "cargo", "prefix restored");
}

#[test]
fn up_with_empty_buffer_walks_all_history() {
    let mut core = core(&["one", "two"], None);
    assert_eq!(core.handle_key(Key::Up), CoreAction::Continue);
    assert_eq!(buffer_str(&core), "two");
    assert_eq!(core.handle_key(Key::Up), CoreAction::Continue);
    assert_eq!(buffer_str(&core), "one");
}

#[test]
fn editing_keys_modify_buffer() {
    let mut core = core(&[], None);
    type_str(&mut core, "hello world");
    feed(&mut core, &[Key::CtrlA, Key::CtrlK]);
    assert_eq!(buffer_str(&core), "");
    type_str(&mut core, "abc def");
    assert_eq!(core.handle_key(Key::CtrlW), CoreAction::Continue);
    assert_eq!(buffer_str(&core), "abc ");
    assert_eq!(core.handle_key(Key::CtrlU), CoreAction::Continue);
    assert_eq!(buffer_str(&core), "");
}

#[test]
fn ctrl_d_semantics() {
    let mut core = core(&[], None);
    assert_eq!(core.handle_key(Key::CtrlD), CoreAction::Eof);
    type_str(&mut core, "ab");
    feed(&mut core, &[Key::Left(false), Key::Left(false)]);
    assert_eq!(core.handle_key(Key::CtrlD), CoreAction::Continue);
    assert_eq!(buffer_str(&core), "b");
}

#[test]
fn ctrl_c_interrupts_and_ctrl_l_clears() {
    let mut core = core(&[], None);
    assert_eq!(core.handle_key(Key::CtrlC), CoreAction::Interrupt);
    assert_eq!(core.handle_key(Key::CtrlL), CoreAction::ClearScreen);
}

#[test]
fn suggestion_lifecycle_through_word_kill() {
    let mut core = core(&["cargo nextest run"], None);
    type_str(&mut core, "cargo nextest run extra");
    // Delete the last word with Ctrl-W; the prefix suggests again.
    assert_eq!(core.handle_key(Key::CtrlW), CoreAction::Continue);
    assert_eq!(buffer_str(&core), "cargo nextest run ");
    assert_eq!(core.suggestion_suffix(), None);
}

#[test]
fn cursor_reposition_width_counts_wide_chars() {
    let mut core = core(&[], None);
    type_str(&mut core, "你好");
    feed(&mut core, &[Key::Left(false), Key::Left(false)]);
    assert_eq!(core.width_right_of_cursor(), 4, "CJK chars are width 2");
}

#[test]
fn paste_inserts_whole_and_flattens_newlines() {
    let mut core = core(&[], None);
    core.insert_paste("hello\nworld\r\ntail");
    assert_eq!(buffer_str(&core), "hello world tail");
    assert_eq!(core.cursor, "hello world tail".chars().count());
}

#[test]
fn alt_word_keys_edit_by_word() {
    let mut core = core(&[], None);
    type_str(&mut core, "hello world");
    assert_eq!(core.handle_key(Key::AltB), CoreAction::Continue);
    assert_eq!(core.cursor, 6);
    assert_eq!(core.handle_key(Key::AltD), CoreAction::Continue);
    assert_eq!(buffer_str(&core), "hello ");
}

#[test]
fn alt_backspace_kills_word_backward() {
    let mut core = core(&[], None);
    type_str(&mut core, "hello world");
    assert_eq!(core.handle_key(Key::AltBackspace), CoreAction::Continue);
    assert_eq!(buffer_str(&core), "hello ");
}

#[test]
fn alt_dot_inserts_history_tail_word() {
    let mut core = core(&["git commit -m msg"], Some("git status"));
    assert_eq!(core.handle_key(Key::AltDot), CoreAction::Continue);
    assert_eq!(buffer_str(&core), "status");
}

#[test]
fn wrap_layout_counts_rows_by_width() {
    let mut core = core(&[], None);
    core.set_term_cols(10);
    type_str(&mut core, "123456789012345");
    assert!(core.full_line_width() > 10);
    assert_eq!(core.width_left_of_cursor(), 2 + 15);
}
