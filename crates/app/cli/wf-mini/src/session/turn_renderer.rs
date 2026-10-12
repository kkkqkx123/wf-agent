//! Turn rendering for the mini session: assistant deltas append to stdout,
//! everything else goes to stderr, via an injectable [`TurnSink`].

use wf_types::events::ExecutionStreamEvent;
use wf_cli_shared::display::MarkdownMode;

use super::TurnOutcome;
use crate::display::DisplayConfig;
use crate::output::{self, AppendWriter};
use crate::render::{strip_terminal_escapes, StreamRenderer};

/// A turn that ran to completion of its stream.
pub(super) struct FinishedTurn {
    pub(super) outcome: TurnOutcome,
}

/// Shorten an execution id for the summary line. Remote turns carry a
/// local `remote:`-prefixed correlation id (not a server execution id),
/// so the prefix is preserved while the random suffix is truncated.
pub(super) fn short_execution_id(execution_id: &str) -> String {
    match execution_id.strip_prefix("remote:") {
        Some(rest) => format!("remote:{}", rest.chars().take(8).collect::<String>()),
        None => execution_id.chars().take(8).collect::<String>(),
    }
}

/// Destination for rendered text, split by the stdout discipline:
/// assistant text goes to stdout, everything else to stderr.
pub(super) trait TurnSink {
    fn stdout_text(&mut self, text: &str);
    fn stderr_text(&mut self, text: &str);
    fn stderr_line(&mut self, text: &str);
}

/// Production sink: the real standard output and error.
#[derive(Debug, Default)]
pub(super) struct RealSink;

impl TurnSink for RealSink {
    fn stdout_text(&mut self, text: &str) {
        let _ = output::write_stdout(text);
    }

    fn stderr_text(&mut self, text: &str) {
        let _ = output::write_stderr(text);
    }

    fn stderr_line(&mut self, text: &str) {
        output::diag_line(text);
    }
}

/// Renders one turn: assistant deltas append to stdout, everything else
/// goes to stderr. Terminal events return whether the turn completed; the
/// full assistant text is kept for the transcript.
///
/// `Raw` markdown streams sanitized text verbatim through the line-buffered
/// [`AppendWriter`]; `Plain`/`Styled` render through a [`StreamRenderer`]
/// that emits complete lines only. The transcript always keeps the
/// sanitized raw text so display choices never leak into model context.
/// Failures always reach stderr even when their stream is hidden.
///
/// The sink is generic so tests can capture every byte without touching
/// real file descriptors; production uses [`RealSink`].
pub(super) struct TurnRenderer<S: TurnSink = RealSink> {
    append: AppendWriter,
    stream: StreamRenderer,
    display: DisplayConfig,
    line_pending: bool,
    assistant_text: String,
    sink: S,
}

impl<S: TurnSink + Default> TurnRenderer<S> {
    pub(super) fn new(display: DisplayConfig) -> Self {
        let mode = display.markdown;
        let color = display.color;
        Self {
            append: AppendWriter::new(),
            stream: StreamRenderer::new(mode, color),
            display,
            line_pending: false,
            assistant_text: String::new(),
            sink: S::default(),
        }
    }

    /// Take the accumulated assistant text for the transcript. Rendering
    /// state (stdout flushing) is untouched.
    pub(super) fn assistant_text(&mut self) -> String {
        std::mem::take(&mut self.assistant_text)
    }

    pub(super) fn on_event(&mut self, event: &ExecutionStreamEvent) -> Option<bool> {
        match event {
            ExecutionStreamEvent::Engine(_) => None,
            ExecutionStreamEvent::IterationStart { .. }
            | ExecutionStreamEvent::IterationEnd { .. } => {
                self.flush_stdout();
                None
            }
            ExecutionStreamEvent::LlmDelta { content } => {
                let clean = strip_terminal_escapes(content);
                self.assistant_text.push_str(&clean);
                let ready = match self.display.markdown {
                    MarkdownMode::Raw => self.append.push(&clean),
                    MarkdownMode::Plain | MarkdownMode::Styled => self.stream.push(&clean),
                };
                if !ready.is_empty() {
                    self.sink.stdout_text(&ready);
                    self.line_pending = !ready.ends_with('\n');
                }
                None
            }
            ExecutionStreamEvent::ToolStart { tool_name, .. } => {
                if !self.display.show_tools {
                    return None;
                }
                self.end_stdout_line();
                self.sink.stderr_line(&format!("tool start: {tool_name}"));
                None
            }
            ExecutionStreamEvent::ToolEnd {
                tool_name, success, ..
            } => {
                if !*success {
                    self.end_stdout_line();
                    self.sink.stderr_line(&format!("tool failed: {tool_name}"));
                    return None;
                }
                if !self.display.show_tools {
                    return None;
                }
                self.end_stdout_line();
                self.sink.stderr_line(&format!("tool ok: {tool_name}"));
                None
            }
            ExecutionStreamEvent::ReasoningDelta { content } => {
                if !self.display.show_reasoning {
                    return None;
                }
                self.end_stdout_line();
                self.sink.stderr_text(&strip_terminal_escapes(content));
                None
            }
            ExecutionStreamEvent::Usage {
                prompt_tokens,
                completion_tokens,
                cost,
            } => {
                if !self.display.show_usage {
                    return None;
                }
                self.end_stdout_line();
                match cost {
                    Some(value) => self.sink.stderr_line(&format!(
                        "usage: {prompt_tokens} prompt + {completion_tokens} completion tokens (~${value:.4})"
                    )),
                    None => self.sink.stderr_line(&format!(
                        "usage: {prompt_tokens} prompt + {completion_tokens} completion tokens"
                    )),
                }
                None
            }
            ExecutionStreamEvent::SubAgentStarted { name, .. } => {
                if !self.display.show_tools {
                    return None;
                }
                self.end_stdout_line();
                self.sink.stderr_line(&format!("subagent started: {name}"));
                None
            }
            ExecutionStreamEvent::SubAgentEnded { name, success, .. } => {
                if !*success {
                    self.end_stdout_line();
                    self.sink.stderr_line(&format!("subagent failed: {name}"));
                    return None;
                }
                if !self.display.show_tools {
                    return None;
                }
                self.end_stdout_line();
                self.sink.stderr_line(&format!("subagent done: {name}"));
                None
            }
            ExecutionStreamEvent::Completed { iterations, .. } => {
                self.flush_stdout();
                if self.display.show_tools {
                    self.sink
                        .stderr_line(&format!("completed in {iterations} iterations"));
                }
                Some(true)
            }
            ExecutionStreamEvent::Failed { error } => {
                self.flush_stdout();
                self.sink
                    .stderr_line(&format!("failed: {}", strip_terminal_escapes(error)));
                Some(false)
            }
            ExecutionStreamEvent::Interrupted { reason } => {
                self.flush_stdout();
                self.sink
                    .stderr_line(&format!("interrupted: {}", strip_terminal_escapes(reason)));
                Some(false)
            }
        }
    }

    fn flush_stdout(&mut self) {
        let remaining = match self.display.markdown {
            MarkdownMode::Raw => self.append.take_remaining(),
            MarkdownMode::Plain | MarkdownMode::Styled => self.stream.flush(),
        };
        if !remaining.is_empty() {
            self.sink.stdout_text(&remaining);
            self.line_pending = !remaining.ends_with('\n');
        }
        self.end_stdout_line();
    }

    fn end_stdout_line(&mut self) {
        if self.line_pending {
            self.sink.stdout_text("\n");
            self.line_pending = false;
        }
    }

    pub(super) fn finish(&mut self) {
        self.flush_stdout();
        let _ = std::io::Write::flush(&mut std::io::stderr());
    }
}

#[cfg(test)]
mod tests {
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
        let (terminal, stdout, stderr, _) =
            render(vec![delta("abc"), tool_start(), tool_end(true)]);
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
        }]);
        assert_eq!(stderr, "usage: 10 prompt + 20 completion tokens\n");
        let (_, _, stderr, _) = render(vec![ExecutionStreamEvent::Usage {
            prompt_tokens: 10,
            completion_tokens: 20,
            cost: Some(0.001),
        }]);
        assert!(stderr.contains("usage: 10 prompt + 20 completion tokens (~$0.0010)"));
    }

    #[test]
    fn subagent_lifecycle_goes_to_stderr() {
        let (_, stdout, stderr, _) = render(vec![
            ExecutionStreamEvent::SubAgentStarted {
                id: "s1".to_string(),
                name: "helper".to_string(),
            },
            ExecutionStreamEvent::SubAgentEnded {
                id: "s1".to_string(),
                name: "helper".to_string(),
                success: false,
            },
        ]);
        assert_eq!(stdout, "");
        assert_eq!(
            stderr,
            "subagent started: helper\nsubagent failed: helper\n"
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
                },
                tool_start(),
                tool_end(true),
                tool_end(false),
            ],
        );
        assert_eq!(stdout, "abc\n");
        assert_eq!(stderr, "tool failed: write_file\n");
    }
}
