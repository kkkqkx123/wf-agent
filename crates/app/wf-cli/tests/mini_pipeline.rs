//! Integration smoke for the mini rendering kernel: synthetic execution
//! stream events drive the exact same reducer → footer → view path as a
//! real mini session, so these assertions cover the wiring without a TTY.

use serde_json::json;

use wf_api::infra::stream::ExecutionStreamEvent;
use wf_api::ToolApprovalRequest;
use wf_cli::approval::ApprovalView;
use wf_cli::footer::{Footer, FooterView};
use wf_cli::keymap::KeymapContext;
use wf_cli::question::QuestionView;
use wf_cli::reducer::{MiniCommit, Phase, SessionReducer};

/// One iteration of markdown streaming that completes.
fn markdown_script() -> Vec<ExecutionStreamEvent> {
    vec![
        ExecutionStreamEvent::IterationStart {
            iteration: 1,
            message_count: 0,
            array_version: 0,
        },
        ExecutionStreamEvent::LlmDelta {
            content: "# demo reply\n\nstreaming through the mini pipeline".to_string(),
        },
        ExecutionStreamEvent::IterationEnd {
            iteration: 1,
            message_count: 0,
            array_version: 0,
        },
        ExecutionStreamEvent::Completed {
            result: serde_json::Value::Null,
            iterations: 1,
        },
    ]
}

/// An assistant preamble, one tool round-trip and the final answer.
fn tool_script() -> Vec<ExecutionStreamEvent> {
    let it = |iteration: u32| ExecutionStreamEvent::IterationStart {
        iteration,
        message_count: 0,
        array_version: 0,
    };
    let it_end = |iteration: u32| ExecutionStreamEvent::IterationEnd {
        iteration,
        message_count: 0,
        array_version: 0,
    };
    vec![
        it(1),
        ExecutionStreamEvent::LlmDelta {
            content: "checking the workspace…\n".to_string(),
        },
        ExecutionStreamEvent::ToolStart {
            tool_call_id: "call-1".to_string(),
            tool_name: "list_files".to_string(),
        },
        ExecutionStreamEvent::ToolEnd {
            tool_call_id: "call-1".to_string(),
            tool_name: "list_files".to_string(),
            success: true,
            result: String::new(),
        },
        it_end(1),
        it(2),
        ExecutionStreamEvent::LlmDelta {
            content: "found 3 files, nothing else to do.".to_string(),
        },
        it_end(2),
        ExecutionStreamEvent::Completed {
            result: serde_json::Value::Null,
            iterations: 2,
        },
    ]
}

#[test]
fn markdown_script_reduces_to_text_and_completion() {
    let mut reducer = SessionReducer::new("exec-1");
    let commits = reducer.push_batch(&markdown_script());
    assert!(commits.iter().any(|c| matches!(
        c,
        MiniCommit::AssistantText { content } if content.contains("demo reply")
    )));
    assert!(commits
        .iter()
        .any(|c| matches!(c, MiniCommit::Completed { iterations: 1 })));
    // After Completed the footer falls back to Idle.
    assert_eq!(reducer.footer().phase, Phase::Idle);
    assert!(reducer.footer().message_count > 0);
}

#[test]
fn tool_script_reduces_tool_lifecycle() {
    let mut reducer = SessionReducer::new("exec-1");
    let commits = reducer.push_batch(&tool_script());
    assert!(commits.iter().any(|c| matches!(
        c,
        MiniCommit::ToolStart { tool_name } if tool_name == "list_files"
    )));
    assert!(commits.iter().any(|c| matches!(
        c,
        MiniCommit::ToolEnd { tool_name, success, .. }
            if tool_name == "list_files" && *success
    )));
    assert!(commits
        .iter()
        .any(|c| matches!(c, MiniCommit::Completed { iterations: 2 })));
}

#[test]
fn footer_reflects_reducer_state_and_routes_keys_by_view() {
    let mut footer = Footer::new();
    footer.state.merge_reducer({
        let mut reducer = SessionReducer::new("exec-1");
        reducer.push_batch(&markdown_script());
        &reducer.footer().clone()
    });
    assert!(footer.apply_height_with_width(80) > 0);

    footer.present(FooterView::Prompt);
    assert_eq!(footer.keymap_context(), KeymapContext::Composer);
    footer.present(FooterView::Permission);
    assert_eq!(footer.keymap_context(), KeymapContext::Approval);
    footer.present(FooterView::Question);
    assert_eq!(footer.keymap_context(), KeymapContext::Question);
}

#[test]
fn approval_and_question_requests_render_views() {
    let approval = ApprovalView::new(ToolApprovalRequest {
        tool_call_id: "call-1".to_string(),
        tool_name: "execute_command".to_string(),
        arguments: json!({ "command": "echo test" }),
        interaction_id: "ui-1".to_string(),
        batch_id: None,
        tool_index: None,
        total_tools: None,
        pending_queue: None,
    });
    assert!(approval.title().contains("execute_command"));
    assert!(!approval.arguments_preview(80).is_empty());

    let question = QuestionView::from_request(
        "ui-2",
        &json!({
            "interactionId": "ui-2",
            "prompt": "which report format should be used?",
            "options": ["markdown", "plain text", "json"],
            "multi": false,
            "allowCustom": true,
        }),
    );
    let lines = question.render_lines(80);
    assert!(lines
        .iter()
        .any(|l| l.to_string().contains("which report format")));
    assert!(lines.iter().any(|l| l.to_string().contains("markdown")));
}
