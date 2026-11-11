
use super::*;
use crate::keymap::{CKey, Key};

async fn started_controller() -> InteractiveController {
    let client = wf_client::WfClient::new(wf_client::ClientConfig::new("http://127.0.0.1:1"));
    InteractiveController::start(client, "exec".into()).await
}

fn key(code: CKey) -> Key {
    Key::plain(code)
}

#[tokio::test]
async fn at_opens_the_mention_panel_and_completes_the_reference() {
    let mut ctl = started_controller().await;
    ctl.handle_key(key(CKey::Char('@')));
    assert_eq!(ctl.footer.route, FooterRoute::Mention);
    assert!(ctl.footer.panel.is_some());
    assert_eq!(ctl.footer.composer.content(), "@");

    ctl.panel_mention_files = vec![MentionEntry::new("file", "src/main.rs")];
    ctl.refresh_mention_entries();
    ctl.handle_key(key(CKey::Enter));
    assert_eq!(ctl.footer.route, FooterRoute::Composer);
    assert_eq!(ctl.footer.composer.content(), "@file:src/main.rs ");
}

#[tokio::test]
async fn at_inside_a_word_does_not_open_the_mention_panel() {
    let mut ctl = started_controller().await;
    for c in ['a', '@', 'b'] {
        ctl.handle_key(key(CKey::Char(c)));
    }
    assert_eq!(ctl.footer.route, FooterRoute::Composer);
}

#[tokio::test]
async fn escape_dismisses_the_mention_panel_for_the_token() {
    let mut ctl = started_controller().await;
    ctl.handle_key(key(CKey::Char('@')));
    assert_eq!(ctl.footer.route, FooterRoute::Mention);
    ctl.handle_key(key(CKey::Esc));
    assert_eq!(ctl.footer.route, FooterRoute::Composer);
    // Typing more keeps the panel closed until the token is gone.
    ctl.handle_key(key(CKey::Char('x')));
    assert_eq!(ctl.footer.route, FooterRoute::Composer);
}

#[tokio::test]
async fn slash_opens_the_command_palette_on_an_empty_composer() {
    let mut ctl = started_controller().await;
    ctl.handle_key(key(CKey::Char('/')));
    assert_eq!(ctl.footer.route, FooterRoute::Command);
    ctl.handle_key(key(CKey::Esc));
    assert_eq!(ctl.footer.route, FooterRoute::Composer);
}

#[tokio::test]
async fn slash_inside_text_stays_ordinary_input() {
    let mut ctl = started_controller().await;
    ctl.handle_key(key(CKey::Char('a')));
    ctl.handle_key(key(CKey::Char('/')));
    assert_eq!(ctl.footer.route, FooterRoute::Composer);
    assert_eq!(ctl.footer.composer.content(), "a/");
}

#[tokio::test]
async fn exclusive_panels_keep_the_text_out_of_the_composer() {
    let mut ctl = started_controller().await;
    ctl.handle_key(key(CKey::Char('/')));
    ctl.handle_key(key(CKey::Char('m')));
    assert_eq!(ctl.footer.route, FooterRoute::Command);
    assert!(ctl.footer.composer.is_empty());
}

#[tokio::test]
async fn the_command_palette_runs_the_selected_command() {
    let mut ctl = started_controller().await;
    ctl.handle_key(key(CKey::Char('/')));
    for c in "help".chars() {
        ctl.handle_key(key(CKey::Char(c)));
    }
    let action = ctl.handle_key(key(CKey::Enter));
    assert!(matches!(action, InteractiveAction::ShowHelp));
}

#[tokio::test]
async fn the_model_selection_sets_the_session_override() {
    let mut ctl = started_controller().await;
    ctl.panel_profiles = vec![wf_types::llm::LlmProfile {
        id: "fast".into(),
        name: "fast".into(),
        format: wf_types::llm::LlmFormat::OpenaiChat,
        provider_id: None,
        model: "gpt-4o-mini".into(),
        api_key: None,
        base_url: None,
        parameters: None,
        generation: None,
        timeout: None,
        max_retries: None,
        retry_delay: None,
        headers: None,
        metadata: None,
        tool_call_protocol: None,
        auth_type: None,
        custom_headers: None,
        custom_body: None,
        custom_body_enabled: None,
        query_params: None,
        stream_options: None,
        context_window_size: None,
        max_output_tokens: None,
        proxy: None,
        no_proxy: None,
        circuit_breaker: None,
        prompt_caching: None,
        auth: None,
        compat: None,
        identity: None,
        transport: None,
        env: None,
    }];
    ctl.open_model_panel();
    ctl.handle_key(key(CKey::Enter));
    assert_eq!(ctl.footer.route, FooterRoute::Composer);
    assert_eq!(ctl.model_override.as_deref(), Some("fast"));
    assert_eq!(ctl.model_override_name().as_deref(), Some("gpt-4o-mini"));
}

#[tokio::test]
async fn a_prompt_submitted_during_a_turn_is_queued() {
    let mut ctl = started_controller().await;
    ctl.turn_task = Some(tokio::spawn(async {}));
    for c in ['h', 'i'] {
        ctl.handle_key(key(CKey::Char(c)));
    }
    ctl.handle_key(key(CKey::Enter));
    assert_eq!(ctl.prompt_queue.len(), 1);
    assert_eq!(ctl.prompt_queue.items()[0].text, "hi");
}

#[tokio::test]
async fn a_typed_command_line_runs_the_command() {
    let mut ctl = started_controller().await;
    ctl.footer.composer.set_text("/model");
    ctl.handle_key(key(CKey::Enter));
    assert_eq!(ctl.footer.route, FooterRoute::Model);
    assert!(ctl.footer.composer.is_empty());
}

#[tokio::test]
async fn the_palette_filter_narrows_the_commands() {
    let mut ctl = started_controller().await;
    ctl.handle_key(key(CKey::Char('/')));
    for c in "model".chars() {
        ctl.handle_key(key(CKey::Char(c)));
    }
    ctl.handle_key(key(CKey::Enter));
    assert_eq!(ctl.footer.route, FooterRoute::Model);
}

#[tokio::test]
async fn an_unknown_command_line_is_reported() {
    let mut ctl = started_controller().await;
    ctl.footer.composer.set_text("/nope");
    ctl.handle_key(key(CKey::Enter));
    assert_eq!(ctl.footer.route, FooterRoute::Composer);
    assert!(ctl
        .footer
        .state
        .notice
        .as_ref()
        .is_some_and(|(text, _)| text.contains("unknown command")));
}

#[tokio::test]
async fn typed_tree_and_fork_commands_reach_the_shell() {
    use crate::interactive::InteractiveAction;
    let mut ctl = started_controller().await;
    assert!(matches!(
        ctl.handle_command_line("/tree"),
        Some(InteractiveAction::OpenSessionTree)
    ));
    assert!(matches!(
        ctl.handle_command_line("/fork"),
        Some(InteractiveAction::ForkLatest)
    ));
    assert!(matches!(
        ctl.handle_command_line("/branch"),
        Some(InteractiveAction::ForkLatest)
    ));
}

#[tokio::test]
async fn the_editor_command_requests_the_handoff() {
    let mut ctl = started_controller().await;
    ctl.footer.composer.set_text("draft");
    ctl.footer.composer.end();
    ctl.handle_command_line("/editor");
    assert!(ctl.take_pending_editor());
    assert_eq!(ctl.footer.composer.content(), "draft");
}

#[tokio::test]
async fn the_queued_panel_lists_and_drops_prompts() {
    let mut ctl = started_controller().await;
    ctl.prompt_queue.push("first");
    ctl.prompt_queue.push("second");
    ctl.open_queued_panel();
    assert_eq!(ctl.footer.route, FooterRoute::Queued);
    ctl.handle_key(key(CKey::Char('D')));
    assert_eq!(ctl.prompt_queue.len(), 1);
    assert_eq!(ctl.prompt_queue.items()[0].text, "second");
}

#[tokio::test]
async fn editing_a_queued_prompt_returns_it_to_the_composer() {
    let mut ctl = started_controller().await;
    ctl.prompt_queue.push("draft");
    ctl.open_queued_panel();
    ctl.handle_key(key(CKey::Enter));
    assert_eq!(ctl.footer.route, FooterRoute::Composer);
    assert_eq!(ctl.footer.composer.content(), "draft");
    assert!(ctl.prompt_queue.is_empty());
}

#[test]
fn reply_rows_counts_folded_lines() {
    assert_eq!(reply_rows(""), 1);
    assert!(reply_rows(&"x".repeat(10_000)) > 1);
}
