//! Mini turn routing for demos: one synthetic turn split by stream.
//!
//! Assistant deltas append to stdout while tool lifecycle, reasoning, usage
//! and sub-agent progress go to stderr. Toggles hide benign streams while
//! failures always show.

use crossterm::style::{Attribute, Color, Stylize};
use wf_cli_shared::display::MarkdownMode;
use wf_mini::display::DisplayConfig;
use wf_types::events::ExecutionStreamEvent;

use crate::mini_render::{strip_terminal_escapes, AppendWriter, StreamRenderer};

/// Captured turn output split the way the terminal shows it.
#[derive(Debug, Default)]
pub struct Capture {
    pub stdout: String,
    pub stderr: String,
    pub line_pending: bool,
    pub assistant_text: String,
    pub background_sessions: Vec<String>,
}

impl Capture {
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

/// One turn through the stdout/stderr discipline. The display knobs reuse
/// the real `wf-mini` [`DisplayConfig`] so demos cannot drift from the
/// production settings.
pub struct TurnPreview {
    append: AppendWriter,
    stream: StreamRenderer,
    display: DisplayConfig,
    capture: Capture,
}

impl TurnPreview {
    pub fn new(display: DisplayConfig) -> Self {
        let mode = display.markdown;
        let color = display.color;
        Self {
            append: AppendWriter::new(),
            stream: StreamRenderer::new(mode, color),
            display,
            capture: Capture::default(),
        }
    }

    fn paint(&self, text: &str, failure: bool, dim: bool) -> String {
        if !self.display.stderr_color {
            return text.to_string();
        }
        if failure {
            text.with(Color::Red).to_string()
        } else if dim {
            text.attribute(Attribute::Dim).to_string()
        } else {
            text.to_string()
        }
    }

    pub fn on_event(&mut self, event: &ExecutionStreamEvent) -> Option<bool> {
        match event {
            ExecutionStreamEvent::Engine(_) => None,
            ExecutionStreamEvent::IterationStart { .. }
            | ExecutionStreamEvent::IterationEnd { .. } => {
                self.flush_stdout();
                None
            }
            ExecutionStreamEvent::LlmDelta { content } => {
                let clean = strip_terminal_escapes(content);
                self.capture.assistant_text.push_str(&clean);
                let ready = match self.display.markdown {
                    MarkdownMode::Raw => self.append.push(&clean),
                    MarkdownMode::Plain | MarkdownMode::Styled => self.stream.push(&clean),
                };
                if !ready.is_empty() {
                    let pending = !ready.ends_with('\n');
                    self.capture.stdout_text(&ready);
                    self.capture.line_pending = pending;
                }
                None
            }
            ExecutionStreamEvent::ToolStart { tool_name, .. } => {
                if !self.display.show_tools {
                    return None;
                }
                self.end_stdout_line();
                let line = self.paint(&format!("tool start: {tool_name}"), false, false);
                self.capture.stderr_line(&line);
                None
            }
            ExecutionStreamEvent::ToolEnd {
                tool_name,
                success,
                background,
                ..
            } => {
                if *success {
                    if let Some(marker) = background {
                        if !self
                            .capture
                            .background_sessions
                            .contains(&marker.session_id)
                        {
                            self.capture
                                .background_sessions
                                .push(marker.session_id.clone());
                            self.end_stdout_line();
                            let line = self.paint(
                                &format!(
                                    "background task: {tool_name} running as {}",
                                    marker.session_id
                                ),
                                false,
                                true,
                            );
                            self.capture.stderr_line(&line);
                        }
                    }
                } else {
                    self.end_stdout_line();
                    let line = self.paint(&format!("tool failed: {tool_name}"), true, false);
                    self.capture.stderr_line(&line);
                    return None;
                }
                if !self.display.show_tools {
                    return None;
                }
                self.end_stdout_line();
                let line = self.paint(&format!("tool ok: {tool_name}"), false, true);
                self.capture.stderr_line(&line);
                None
            }
            ExecutionStreamEvent::ReasoningDelta { content } => {
                if !self.display.show_reasoning {
                    return None;
                }
                self.end_stdout_line();
                self.capture.stderr_text(&strip_terminal_escapes(content));
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
                let raw = match cost {
                    Some(value) => format!(
                        "usage: {prompt_tokens} prompt + {completion_tokens} completion tokens (~${value:.4})"
                    ),
                    None => format!(
                        "usage: {prompt_tokens} prompt + {completion_tokens} completion tokens"
                    ),
                };
                let line = self.paint(&raw, false, true);
                self.capture.stderr_line(&line);
                None
            }
            ExecutionStreamEvent::SubAgentStarted { name, .. } => {
                if !self.display.show_tools {
                    return None;
                }
                self.end_stdout_line();
                let line = self.paint(&format!("subagent started: {name}"), false, true);
                self.capture.stderr_line(&line);
                None
            }
            ExecutionStreamEvent::SubAgentEnded { name, success, .. } => {
                if !*success {
                    self.end_stdout_line();
                    let line = self.paint(&format!("subagent failed: {name}"), true, false);
                    self.capture.stderr_line(&line);
                    return None;
                }
                if !self.display.show_tools {
                    return None;
                }
                self.end_stdout_line();
                let line = self.paint(&format!("subagent done: {name}"), false, true);
                self.capture.stderr_line(&line);
                None
            }
            ExecutionStreamEvent::SubAgentProgress { activity, .. } => {
                let text = activity.as_deref().filter(|t| !t.trim().is_empty())?;
                if !self.display.show_tools {
                    return None;
                }
                self.end_stdout_line();
                let line = self.paint(&format!("subagent: {text}"), false, true);
                self.capture.stderr_line(&line);
                None
            }
            ExecutionStreamEvent::Completed { iterations, .. } => {
                self.flush_stdout();
                if !self.capture.background_sessions.is_empty() {
                    let line = self.paint(
                        &format!(
                            "background tasks: {} running ({})",
                            self.capture.background_sessions.len(),
                            self.capture.background_sessions.join(", "),
                        ),
                        false,
                        true,
                    );
                    self.capture.stderr_line(&line);
                }
                if self.display.show_tools {
                    let line = self.paint(
                        &format!("completed in {iterations} iterations"),
                        false,
                        true,
                    );
                    self.capture.stderr_line(&line);
                }
                Some(true)
            }
            ExecutionStreamEvent::Failed { error } => {
                self.flush_stdout();
                let line = self.paint(
                    &format!("failed: {}", strip_terminal_escapes(error)),
                    true,
                    false,
                );
                self.capture.stderr_line(&line);
                Some(false)
            }
            ExecutionStreamEvent::Interrupted { reason } => {
                self.flush_stdout();
                let line = self.paint(
                    &format!("interrupted: {}", strip_terminal_escapes(reason)),
                    true,
                    false,
                );
                self.capture.stderr_line(&line);
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
            let pending = !remaining.ends_with('\n');
            self.capture.stdout_text(&remaining);
            self.capture.line_pending = pending;
        }
        self.end_stdout_line();
    }

    fn end_stdout_line(&mut self) {
        if self.capture.line_pending {
            self.capture.stdout_text("\n");
            self.capture.line_pending = false;
        }
    }

    fn finish(&mut self) {
        self.flush_stdout();
    }
}

/// Representative turn: markdown deltas, one tool, reasoning, usage, sub-agent.
pub fn demo_script() -> Vec<ExecutionStreamEvent> {
    vec![
        ExecutionStreamEvent::LlmDelta {
            content: "Fixing login with **back".to_string(),
        },
        ExecutionStreamEvent::LlmDelta {
            content: "off** retry\n".to_string(),
        },
        ExecutionStreamEvent::ToolStart {
            tool_call_id: "t1".to_string(),
            tool_name: "read_file".to_string(),
        },
        ExecutionStreamEvent::ReasoningDelta {
            content: "checking caller".to_string(),
        },
        ExecutionStreamEvent::ToolEnd {
            tool_call_id: "t1".to_string(),
            tool_name: "read_file".to_string(),
            success: true,
            result: String::new(),
            error: None,
            display: None,
            background: None,
        },
        ExecutionStreamEvent::LlmDelta {
            content: "patched `login()`\n".to_string(),
        },
        ExecutionStreamEvent::Usage {
            prompt_tokens: 120,
            completion_tokens: 45,
            cost: Some(0.0012),
        },
        ExecutionStreamEvent::SubAgentStarted {
            id: "s1".to_string(),
            name: "helper".to_string(),
            tool_call_id: Some("t2".to_string()),
            depth: Some(1),
            preview: Some("verify fix".to_string()),
            background: Some(false),
        },
        ExecutionStreamEvent::SubAgentProgress {
            id: "s1".to_string(),
            tool_call_id: Some("t2".to_string()),
            activity: Some("running tests".to_string()),
            output_tokens: None,
            tool_uses: Some(2),
        },
        ExecutionStreamEvent::SubAgentEnded {
            id: "s1".to_string(),
            name: "helper".to_string(),
            success: true,
            tool_call_id: Some("t2".to_string()),
            background: Some(false),
        },
        ExecutionStreamEvent::Completed {
            result: serde_json::Value::Null,
            iterations: 2,
        },
    ]
}

/// Drive one scripted turn to its terminal event.
pub fn run_turn(display: DisplayConfig, events: &[ExecutionStreamEvent]) -> Capture {
    let mut turn = TurnPreview::new(display);
    for event in events {
        if turn.on_event(event).is_some() {
            break;
        }
    }
    turn.finish();
    turn.capture
}

/// Annotated `out|` / `err|` view of one captured turn.
pub fn annotated(capture: &Capture) -> String {
    let mut out = String::new();
    for line in capture.stdout.split_inclusive('\n') {
        out.push_str("out| ");
        out.push_str(line);
        if !line.ends_with('\n') {
            out.push('\n');
        }
    }
    for line in capture.stderr.split_inclusive('\n') {
        out.push_str("err| ");
        out.push_str(line);
        if !line.ends_with('\n') {
            out.push('\n');
        }
    }
    out.push_str(&format!("transcript-text: {:?}\n", capture.assistant_text));
    out
}
