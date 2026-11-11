use super::*;

#[test]
fn shortens_execution_id() {
    assert_eq!(short_execution_id("abcdef123456"), "abcdef12");
}

#[test]
fn preserves_remote_correlation_prefix() {
    assert_eq!(short_execution_id("remote:abcdef123456"), "remote:abcdef12");
}

/// Capturing sink: every byte the renderer assigns to stdout/stderr
/// lands here instead of real file descriptors.
#[derive(Debug, Default)]
struct TestSink {
    stdout: String,
    stderr: String,
}

impl TurnSink for TestSink {
    fn stdout_text(&mut self, text: &str) {
        self.stdout.push_str(text);
    }

    fn stderr_text(&mut self, text: &str) {
        self.stderr.push_str(text);
    }

    fn stderr_line(&mut self, text: &str) {
        self.stderr.push_str(text);
        self.stderr.push('\n');
    }
}

fn delta(text: &str) -> ExecutionStreamEvent {
    ExecutionStreamEvent::LlmDelta {
        content: text.to_string(),
    }
}

fn tool_start() -> ExecutionStreamEvent {
    ExecutionStreamEvent::ToolStart {
        tool_call_id: "t1".to_string(),
        tool_name: "write_file".to_string(),
    }
}

fn tool_end(success: bool) -> ExecutionStreamEvent {
    ExecutionStreamEvent::ToolEnd {
        tool_call_id: "t1".to_string(),
        tool_name: "write_file".to_string(),
        success,
        result: String::new(),
        error: None,
        display: None,
        background: None,
    }
}

fn engine_event() -> ExecutionStreamEvent {
    ExecutionStreamEvent::Engine(wf_types::events::BaseEvent {
        id: "e".into(),
        r#type: wf_types::events::EventType::MessageAdded,
        timestamp: 0,
        event_name: None,
        workflow_id: None,
        execution_id: None,
        agent_loop_id: None,
        metadata: None,
    })
}

fn iteration_start() -> ExecutionStreamEvent {
    ExecutionStreamEvent::IterationStart {
        iteration: 1,
        message_count: 0,
        array_version: 0,
    }
}

fn completed() -> ExecutionStreamEvent {
    ExecutionStreamEvent::Completed {
        result: serde_json::Value::Null,
        iterations: 2,
    }
}

/// Feed events through a capturing renderer to completion: returns
/// (terminal outcome, stdout, stderr, assistant text).
fn render(events: Vec<ExecutionStreamEvent>) -> (Option<bool>, String, String, String) {
    render_with(DisplayConfig::default(), events)
}

fn render_with(
    display: DisplayConfig,
    events: Vec<ExecutionStreamEvent>,
) -> (Option<bool>, String, String, String) {
    let mut renderer = TurnRenderer::<TestSink>::new(display);
    let mut terminal = None;
    for event in &events {
        if let Some(done) = renderer.on_event(event) {
            terminal = Some(done);
            break;
        }
    }
    renderer.finish();
    let text = renderer.assistant_text();
    (
        terminal,
        std::mem::take(&mut renderer.sink.stdout),
        std::mem::take(&mut renderer.sink.stderr),
        text,
    )
}

#[test]
fn assistant_deltas_stream_to_stdout_verbatim() {
    let (terminal, stdout, stderr, text) = render(vec![
        delta("hello "),
        delta("world\n"),
        delta("tail"),
        completed(),
    ]);
    assert_eq!(terminal, Some(true));
    assert_eq!(stdout, "hello world\ntail\n");
    assert_eq!(stderr, "completed in 2 iterations\n");
    assert_eq!(text, "hello world\ntail");
}

#[test]
fn partial_line_closed_by_tool_event() {
    let (terminal, stdout, stderr, _) = render(vec![delta("abc"), tool_start(), tool_end(true)]);
    assert_eq!(terminal, None);
    assert_eq!(stdout, "abc\n");
    assert_eq!(stderr, "tool start: write_file\ntool ok: write_file\n");
}

#[test]
fn tool_failure_reported_on_stderr() {
    let (_, stdout, stderr, _) = render(vec![tool_end(false)]);
    assert_eq!(stdout, "");
    assert_eq!(stderr, "tool failed: write_file\n");
}

#[test]
fn background_tool_end_prints_a_follow_up_hint() {
    let (_, stdout, stderr, _) = render(vec![ExecutionStreamEvent::ToolEnd {
        tool_call_id: "t1".to_string(),
        tool_name: "backend_shell".to_string(),
        success: true,
        result: String::new(),
        error: None,
        display: None,
        background: Some(wf_types::tool::BackgroundMarker {
            session_id: "sess-1".to_string(),
            backgrounded: true,
        }),
    }]);
    assert_eq!(stdout, "");
    assert!(
        stderr.contains("background task: backend_shell running as sess-1"),
        "{stderr}"
    );
}

#[test]
fn completed_prints_background_aggregate() {
    let (_, _, stderr, _) = render(vec![
        ExecutionStreamEvent::ToolEnd {
            tool_call_id: "t1".to_string(),
            tool_name: "backend_shell".to_string(),
            success: true,
            result: String::new(),
            error: None,
            display: None,
            background: Some(wf_types::tool::BackgroundMarker {
                session_id: "sess-1".to_string(),
                backgrounded: true,
            }),
        },
        ExecutionStreamEvent::ToolEnd {
            tool_call_id: "t2".to_string(),
            tool_name: "shell_wait".to_string(),
            success: true,
            result: String::new(),
            error: None,
            display: None,
            background: Some(wf_types::tool::BackgroundMarker {
                session_id: "sess-2".to_string(),
                backgrounded: true,
            }),
        },
        completed(),
    ]);
    assert!(stderr.contains("background tasks: 2 running"), "{stderr}");
    assert!(stderr.contains("shell_ack"), "{stderr}");
}

#[test]
fn reasoning_goes_to_stderr_without_newline() {
    let (_, stdout, stderr, text) = render(vec![
        delta("abc"),
        ExecutionStreamEvent::ReasoningDelta {
            content: "thinking".to_string(),
        },
    ]);
    assert_eq!(stdout, "abc\n");
    assert_eq!(stderr, "thinking");
    assert_eq!(text, "abc");
}

#[test]
fn usage_lines_go_to_stderr() {
    let (_, _, stderr, _) = render(vec![ExecutionStreamEvent::Usage {
        prompt_tokens: 10,
        completion_tokens: 20,
        cost: None,
        reasoning_tokens: None,
        cache_read_tokens: None,
        cache_write_tokens: None,
        total_tokens: None,
    }]);
    assert_eq!(stderr, "usage: 10 prompt + 20 completion tokens\n");
    let (_, _, stderr, _) = render(vec![ExecutionStreamEvent::Usage {
        prompt_tokens: 10,
        completion_tokens: 20,
        cost: Some(0.001),
        reasoning_tokens: None,
        cache_read_tokens: None,
        cache_write_tokens: None,
        total_tokens: None,
    }]);
    assert!(stderr.contains("usage: 10 prompt + 20 completion tokens (~$0.0010)"));
}

#[test]
fn usage_line_annotates_reported_cache() {
    let (_, _, stderr, _) = render(vec![ExecutionStreamEvent::Usage {
        prompt_tokens: 10,
        completion_tokens: 20,
        cost: None,
        reasoning_tokens: None,
        cache_read_tokens: Some(7),
        cache_write_tokens: None,
        total_tokens: None,
    }]);
    assert_eq!(
        stderr,
        "usage: 10 prompt + 20 completion tokens (7 cached)\n"
    );
}

#[test]
fn subagent_lifecycle_goes_to_stderr() {
    let (_, stdout, stderr, _) = render(vec![
        ExecutionStreamEvent::SubAgentStarted {
            id: "s1".to_string(),
            name: "helper".to_string(),
            tool_call_id: Some("t9".to_string()),
            depth: Some(1),
            preview: Some("help out".to_string()),
            background: Some(false),
        },
        ExecutionStreamEvent::SubAgentProgress {
            id: "s1".to_string(),
            tool_call_id: Some("t9".to_string()),
            activity: Some("reading files".to_string()),
            output_tokens: None,
            tool_uses: Some(2),
        },
        ExecutionStreamEvent::SubAgentEnded {
            id: "s1".to_string(),
            name: "helper".to_string(),
            success: false,
            tool_call_id: Some("t9".to_string()),
            background: Some(false),
        },
    ]);
    assert_eq!(stdout, "");
    assert_eq!(
        stderr,
        "subagent started: helper\nsubagent: reading files\nsubagent failed: helper\n"
    );
}

#[test]
fn failed_and_interrupted_flush_and_terminate() {
    let (terminal, stdout, stderr, _) = render(vec![
        delta("abc"),
        ExecutionStreamEvent::Failed {
            error: "boom".to_string(),
        },
    ]);
    assert_eq!(terminal, Some(false));
    assert_eq!(stdout, "abc\n");
    assert_eq!(stderr, "failed: boom\n");

    let (terminal, stdout, stderr, _) = render(vec![
        delta("abc"),
        ExecutionStreamEvent::Interrupted {
            reason: "user".to_string(),
        },
    ]);
    assert_eq!(terminal, Some(false));
    assert_eq!(stdout, "abc\n");
    assert_eq!(stderr, "interrupted: user\n");
}

#[test]
fn engine_and_iteration_events_leave_stdout_clean() {
    let (terminal, stdout, stderr, _) = render(vec![engine_event(), iteration_start()]);
    assert_eq!(terminal, None);
    assert_eq!(stdout, "");
    assert_eq!(stderr, "");
}

#[test]
fn iteration_boundary_flushes_pending_partial_line() {
    let (_, stdout, _, _) = render(vec![delta("abc"), iteration_start()]);
    assert_eq!(stdout, "abc\n");
}

#[test]
fn completed_without_deltas_writes_no_stdout() {
    let (terminal, stdout, stderr, text) = render(vec![completed()]);
    assert_eq!(terminal, Some(true));
    assert_eq!(stdout, "");
    assert_eq!(stderr, "completed in 2 iterations\n");
    assert_eq!(text, "");
}

#[test]
fn plain_mode_strips_markup_but_keeps_transcript_raw() {
    let display = DisplayConfig {
        markdown: MarkdownMode::Plain,
        ..DisplayConfig::default()
    };
    let (terminal, stdout, stderr, text) = render_with(
        display,
        vec![delta("**hello** world\n"), delta("tail"), completed()],
    );
    assert_eq!(terminal, Some(true));
    assert_eq!(stdout, "hello world\ntail\n");
    assert_eq!(stderr, "completed in 2 iterations\n");
    assert_eq!(text, "**hello** world\ntail");
}

#[test]
fn styled_mode_without_color_degrades_to_plain() {
    let display = DisplayConfig {
        markdown: MarkdownMode::Styled,
        color: false,
        ..DisplayConfig::default()
    };
    let (_, stdout, _, _) = render_with(display, vec![delta("**hi**\n"), completed()]);
    assert_eq!(stdout, "hi\n");
    assert!(!stdout.contains('\x1b'));
}

#[test]
fn styled_mode_with_color_emits_sgr() {
    let display = DisplayConfig {
        markdown: MarkdownMode::Styled,
        color: true,
        ..DisplayConfig::default()
    };
    let (_, stdout, _, _) = render_with(display, vec![delta("**hi**\n"), completed()]);
    assert!(stdout.contains("\x1b[1m"), "{stdout:?}");
    assert!(stdout.contains("\x1b[0m"), "{stdout:?}");
}

#[test]
fn model_escapes_never_reach_stdout() {
    let (_, stdout, _, text) = render(vec![delta("\x1b[2Jhi\x1b[0m\n"), completed()]);
    assert_eq!(stdout, "hi\n");
    assert_eq!(text, "hi\n");
}

#[test]
fn hidden_streams_stay_silent_but_failures_show() {
    let display = DisplayConfig {
        show_reasoning: false,
        show_usage: false,
        show_tools: false,
        ..DisplayConfig::default()
    };
    let (_, stdout, stderr, _) = render_with(
        display,
        vec![
            delta("abc"),
            ExecutionStreamEvent::ReasoningDelta {
                content: "thinking".to_string(),
            },
            ExecutionStreamEvent::Usage {
                prompt_tokens: 1,
                completion_tokens: 2,
                cost: None,
                reasoning_tokens: None,
                cache_read_tokens: None,
                cache_write_tokens: None,
                total_tokens: None,
            },
            tool_start(),
            tool_end(true),
            tool_end(false),
        ],
    );
    assert_eq!(stdout, "abc\n");
    assert_eq!(stderr, "tool failed: write_file\n");
}

#[test]
fn stderr_color_styles_failures_and_dims_success() {
    let display = DisplayConfig {
        stderr_color: true,
        ..DisplayConfig::default()
    };
    let (_, _, stderr, _) = render_with(display, vec![tool_end(false)]);
    assert!(stderr.contains("\x1b["), "{stderr:?}");
    assert!(stderr.contains("tool failed"), "{stderr:?}");
    assert_eq!(
        crate::render::strip_terminal_escapes(&stderr),
        "tool failed: write_file\n"
    );
    let (_, _, stderr, _) = render_with(display, vec![tool_end(true)]);
    assert!(stderr.contains("\x1b["), "{stderr:?}");
    assert_eq!(
        crate::render::strip_terminal_escapes(&stderr),
        "tool ok: write_file\n"
    );
}

#[test]
fn stderr_without_color_has_no_escapes() {
    let display = DisplayConfig {
        stderr_color: false,
        ..DisplayConfig::default()
    };
    let (_, _, stderr, _) = render_with(display, vec![tool_end(false)]);
    assert!(!stderr.contains('\x1b'), "{stderr:?}");
}
