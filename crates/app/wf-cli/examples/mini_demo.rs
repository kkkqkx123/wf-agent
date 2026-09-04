//! Mini pipeline showcase: drives the exact same rendering kernel as
//! `wf --mini` (protocol events → `SessionReducer` footer state, markdown
//! streaming → scrollback lines, approval / question views) with a
//! synthetic event script — no runtime, no TTY. The script speaks only
//! the execution stream protocol (`wf_api::ExecutionStreamEvent`): a
//! client never sees engine-internal event types.
//!
//! Run with: `cargo run -p wf-cli --example mini_demo`

use serde_json::json;

use wf_api::infra::stream::ExecutionStreamEvent;
use wf_api::ToolApprovalRequest;
use wf_cli::approval::ApprovalView;
use wf_cli::footer::{Footer, FooterView};
use wf_cli::markdown::MarkdownStream;
use wf_cli::question::QuestionView;
use wf_cli::reducer::{MiniCommit, SessionReducer};
use wf_cli::scrollback::{HistoryLine, Role};

/// The markdown document streamed through the pipeline (heading + list +
/// table + fenced code — exercises the table holdback and fence paths).
const MARKDOWN_REPLY: &str = "\
# showcase reply

Streaming markdown through the mini pipeline:

- committed blocks settle into the scrollback
- the in-flight tail renders live
- tables are held back until the stream finalizes

| feature | state |
| :------ | :---- |
| streaming | live |
| tables | held back |

```rust
fn main() {
    println!(\"hello from the showcase\");
}
```
";

/// Split text into word-boundary chunks to emulate LLM delta delivery.
fn chunked(text: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut buf = String::new();
    for word in text.split(' ') {
        if !buf.is_empty() {
            buf.push(' ');
        }
        buf.push_str(word);
        if buf.len() >= 24 {
            chunks.push(std::mem::take(&mut buf));
        }
    }
    if !buf.is_empty() {
        chunks.push(buf);
    }
    chunks
}

/// The synthetic execution run: one markdown iteration, one tool
/// round-trip, a final answer, completion.
fn script() -> Vec<ExecutionStreamEvent> {
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
    let mut events = vec![it(1)];
    for chunk in chunked(MARKDOWN_REPLY) {
        events.push(ExecutionStreamEvent::LlmDelta { content: chunk });
    }
    events.push(it_end(1));
    events.push(it(2));
    events.push(ExecutionStreamEvent::ToolStart {
        tool_call_id: "showcase-tool-1".to_string(),
        tool_name: "list_files".to_string(),
    });
    events.push(ExecutionStreamEvent::ToolEnd {
        tool_call_id: "showcase-tool-1".to_string(),
        tool_name: "list_files".to_string(),
        success: true,
        result: "a.txt\nb.txt\nc.txt".to_string(),
    });
    events.push(ExecutionStreamEvent::LlmDelta {
        content: "found 3 files, nothing else to do.".to_string(),
    });
    events.push(it_end(2));
    events.push(ExecutionStreamEvent::Completed {
        result: serde_json::Value::Null,
        iterations: 2,
    });
    events
}

fn describe(commit: &MiniCommit) -> String {
    match commit {
        MiniCommit::User { content } => format!("user: {content}"),
        MiniCommit::AssistantText { content } => {
            format!("assistant text: {} bytes", content.len())
        }
        MiniCommit::ToolStart { tool_name } => format!("tool start: {tool_name}"),
        MiniCommit::ToolEnd { tool_name, success } => {
            format!("tool end: {tool_name} success={success}")
        }
        MiniCommit::IterationBoundary => "iteration boundary".to_string(),
        MiniCommit::Completed { iterations } => format!("completed ({iterations} iterations)"),
        MiniCommit::Failed { error } => format!("failed: {error}"),
        MiniCommit::Interrupted { reason } => format!("interrupted: {reason}"),
    }
}

fn main() {
    let width = 80usize;

    println!("=== mini pipeline showcase ===");
    println!();

    // 1. Agent event stream → SessionReducer → commits + footer state.
    let mut reducer = SessionReducer::new("showcase-execution");
    let mut commits = Vec::new();
    for event in script() {
        commits.extend(reducer.push_batch(std::slice::from_ref(&event)));
    }
    println!("-- reducer commits --");
    for commit in &commits {
        println!("  {}", describe(commit));
    }
    println!();

    // 2. Footer state: the same snapshot the mini statusline renders.
    let mut footer = Footer::new();
    footer.state.merge_reducer(reducer.footer());
    println!(
        "-- footer state -- phase={:?} iterations={} messages={} height={}",
        reducer.footer().phase,
        reducer.footer().iteration,
        reducer.footer().message_count,
        footer.apply_height_with_width(width as u16)
    );
    println!();

    // 3. Markdown streaming → scrollback lines (the mini scrollback path).
    println!("-- scrollback (markdown pipeline) --");
    let mut stream = MarkdownStream::default();
    let mut lines: Vec<HistoryLine> = Vec::new();
    for chunk in chunked(MARKDOWN_REPLY) {
        let frame = stream.push(&chunk);
        if !frame.new_committed.is_empty() {
            lines.push(HistoryLine::new_role(frame.new_committed, Role::Default));
        }
    }
    let tail = stream.finish();
    if !tail.new_committed.is_empty() {
        lines.push(HistoryLine::new_role(tail.new_committed, Role::Default));
    }
    for line in &lines {
        for row in line.display_lines(width as u16) {
            println!("  {row}");
        }
    }
    println!();

    // 4. Tool lifecycle lines (the mini scrollback rendering).
    println!("-- tool lifecycle --");
    println!("  ▲ list_files");
    println!("  ✓ list_files");
    println!();

    // 5. The approval view (y/a/d/n/c keys) as rendered in the footer.
    let approval = ApprovalView::new(ToolApprovalRequest {
        tool_call_id: "showcase-call-1".to_string(),
        tool_name: "execute_command".to_string(),
        arguments: json!({ "command": "echo showcase" }),
        interaction_id: "showcase-interaction-1".to_string(),
        batch_id: None,
        tool_index: None,
        total_tools: None,
        pending_queue: None,
    });
    println!("-- approval view ({:?}) --", FooterView::Permission);
    println!("  {}", approval.title());
    println!("  {}", approval.arguments_preview(width));
    println!("  {}", approval.hints());
    println!();

    // 6. The follow-up question view (single select + custom fallback).
    let question = QuestionView::from_request(
        "showcase-interaction-2",
        &json!({
            "interactionId": "showcase-interaction-2",
            "prompt": "which report format should be used?",
            "options": ["markdown", "plain text", "json"],
            "multi": false,
            "allowCustom": true,
        }),
    );
    println!("-- question view ({:?}) --", FooterView::Question);
    for line in question.render_lines(width) {
        let text = line.to_string();
        if !text.is_empty() {
            println!("  {text}");
        }
    }
}
