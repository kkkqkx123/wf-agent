
use super::*;

#[test]
fn quit_armer_quits_only_on_back_to_back_presses() {
    let start = Instant::now();
    let mut armer = QuitArmer::default();
    assert!(!armer.note_press(start));
    assert!(armer.note_press(start + Duration::from_secs(1)));
}

#[test]
fn quit_armer_ignores_stale_presses() {
    let start = Instant::now();
    let mut armer = QuitArmer::default();
    assert!(!armer.note_press(start));
    assert!(!armer.note_press(start + PROMPT_QUIT_WINDOW + Duration::from_secs(1)));
}

#[test]
fn quit_armer_reset_disarms() {
    let start = Instant::now();
    let mut armer = QuitArmer::default();
    assert!(!armer.note_press(start));
    armer.reset();
    assert!(!armer.note_press(start + Duration::from_secs(1)));
}

fn mini_row(id: &str, title: Option<&str>, status: &str) -> MiniSessionRow {
    MiniSessionRow {
        id: id.to_string(),
        title: title.map(str::to_string),
        status: status.to_string(),
        start_ms: 1,
    }
}

#[test]
fn session_tokens_split_plain_words() {
    let tokens = parse_session_tokens("deploy api");
    assert_eq!(
        tokens,
        vec![
            SessionToken {
                phrase: false,
                value: "deploy".to_string(),
            },
            SessionToken {
                phrase: false,
                value: "api".to_string(),
            },
        ]
    );
    assert!(parse_session_tokens("   ").is_empty());
}

#[test]
fn session_tokens_honor_quoted_phrase() {
    let tokens = parse_session_tokens("fix \"deploy api\" now");
    assert_eq!(tokens.len(), 3);
    assert!(!tokens[0].phrase);
    assert!(tokens[1].phrase);
    assert_eq!(tokens[1].value, "deploy api");
    assert!(!tokens[2].phrase);
}

#[test]
fn session_tokens_unbalanced_quote_falls_back() {
    let tokens = parse_session_tokens("fix \"deploy api");
    assert!(tokens.iter().all(|token| !token.phrase));
    assert_eq!(tokens.len(), 3);
}

#[test]
fn session_row_phrase_matches_across_spacing() {
    let row = mini_row("abc", Some("deploy   API hotfix"), "completed");
    let tokens = parse_session_tokens("\"deploy api\"");
    assert_eq!(session_row_score(&row, &tokens), Some(0));
    let miss = parse_session_tokens("\"api deploy\"");
    assert_eq!(session_row_score(&row, &miss), None);
}

#[test]
fn session_row_tokens_require_all() {
    let row = mini_row("abc", Some("deploy api"), "completed");
    let both = parse_session_tokens("deploy completed");
    assert!(session_row_score(&row, &both).is_some());
    let missing = parse_session_tokens("deploy failed");
    assert_eq!(session_row_score(&row, &missing), None);
}

#[test]
fn filter_session_rows_ranks_title_before_status() {
    let titled = mini_row("id-1", Some("deploy api"), "running");
    let status_hit = mini_row("id-2", Some("other work"), "deploying");
    let rows = vec![&status_hit, &titled];
    let shown = filter_session_rows(rows, &parse_session_tokens("deploy"));
    assert_eq!(shown.len(), 2);
    assert_eq!(shown[0].id, "id-1");
}

#[test]
fn strip_named_flag_parses_prefix() {
    assert_eq!(strip_named_flag(None), (false, None));
    assert_eq!(strip_named_flag(Some("--named".to_string())), (true, None));
    assert_eq!(
        strip_named_flag(Some("--named deploy".to_string())),
        (true, Some("deploy".to_string()))
    );
    assert_eq!(
        strip_named_flag(Some("deploy".to_string())),
        (false, Some("deploy".to_string()))
    );
}

#[test]
fn resolve_numbered_target_uses_display_order() {
    let rows = vec![
        mini_row("id-1", None, "completed"),
        mini_row("id-2", None, "completed"),
    ];
    assert_eq!(resolve_numbered_target("2", &rows).as_deref(), Some("id-2"));
    assert_eq!(resolve_numbered_target("0", &rows), None);
    assert_eq!(resolve_numbered_target("3", &rows), None);
    assert_eq!(resolve_numbered_target("abc", &rows), None);
}
