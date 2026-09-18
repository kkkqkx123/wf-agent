//! Cross-crate invariants of the headless summary kernel
//! (`tui_core::headless::HeadlessRenderer`).
//!
//! These tests live in `tui-render` because it is the only crate that
//! depends on all three participants of the same-source pipeline
//! (reducer commits, streaming markdown, scrollback rendering).

use tui_components::transcript::{lines_to_string, HistoryLine};
use tui_core::headless::HeadlessRenderer;
use tui_core::reducer::{fold, MiniCommit, Phase};
use wf_api::infra::stream::ExecutionStreamEvent;

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
            let line = HistoryLine::new(content.clone());
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

/// End-to-end smoke: feed a real `agent_execution::stream` (mock LLM) into
/// [`HeadlessRenderer`] and verify the stdout/diag split on a real event flow.
mod e2e {
    use super::*;
    use futures::StreamExt;
    use std::collections::HashMap;
    use std::sync::Arc;
    use wf_api::agent::agent_execution::{self, RunAgentLoopParams};
    use wf_api::{AgentLoopConfig, AgentLoopInput};
    use wf_cli_shared::domain::DomainAdapter;
    use wf_llm::{LlmResponseSpec, MockLlmClient};
    use wf_types::Id;

    async fn adapter_with_mock(
        script: Vec<LlmResponseSpec>,
        default: LlmResponseSpec,
    ) -> DomainAdapter {
        let adapter = DomainAdapter::bootstrap(wf_cli_shared::default_runtime_config())
            .await
            .unwrap();
        let mock = Arc::new(MockLlmClient::new());
        for spec in script {
            mock.script(spec);
        }
        mock.default(default);
        adapter.llm_gateway().register_mock("mock", mock.clone());
        adapter
    }

    #[tokio::test]
    async fn headless_renderer_smokes_mock_llm_stream_end_to_end() {
        let adapter = adapter_with_mock(
            vec![LlmResponseSpec::text("hello from headless e2e")],
            LlmResponseSpec::text("fallback"),
        )
        .await;
        let execution_id = wf_common::generate_id();
        let ctx = adapter.api_context();

        let params = RunAgentLoopParams {
            agent_loop_id: Some(Id::from(execution_id.clone())),
            approval_options: None,
            approval_handler: None,
            config: AgentLoopConfig {
                agent_id: Id::from("cli"),
                model: "mock".into(),
                max_iterations: Some(50),
                max_execution_time: None,
                hooks: Vec::new(),
                available_tool_names: Vec::new(),
                initial_tool_names: Vec::new(),
                discoverable_tool_names: Vec::new(),
                enable_general_tool: None,
                activated_tool_names: Vec::new(),
                hidden_tool_names: Vec::new(),
                tool_call_protocol: None,
                token_limit: None,
                token_warning_threshold: None,
                enable_token_tracking: None,
                general_description: None,
                discoverable_metadata_block: None,
                history_normalization: false,
                checkpoint_message_interval: None,
            },
            input: AgentLoopInput {
                message: "hi".into(),
                context: HashMap::new(),
                conversation: Vec::new(),
            },
        };

        let mut stream = agent_execution::stream(ctx, params).await.unwrap();
        let mut renderer = HeadlessRenderer::new(execution_id.clone());
        let mut stdout = String::new();
        let mut diag: Vec<String> = Vec::new();
        let mut completed = false;
        while let Some(event) = stream.next().await {
            let delta = renderer.on_event(&event);
            stdout.push_str(&delta.stdout);
            diag.extend(delta.diag);
            if matches!(event, ExecutionStreamEvent::Completed { .. }) {
                completed = true;
            }
        }
        stdout.push_str(&renderer.finish().stdout);

        assert!(completed, "agent stream must complete");
        assert!(
            stdout.contains("hello from headless e2e"),
            "stdout must carry the mock answer: {stdout:?}"
        );
        assert!(
            diag.is_empty(),
            "text-only run produces no tool diagnostics: {diag:?}"
        );
        assert_eq!(renderer.footer().phase, Phase::Idle);

        adapter.shutdown().await.unwrap();
    }
}
