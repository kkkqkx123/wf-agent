//! Turn rendering for the mini session: assistant deltas append to stdout,
//! everything else goes to stderr, via an injectable [`TurnSink`].

use wf_cli_shared::display::MarkdownMode;
use wf_types::events::ExecutionStreamEvent;

use crossterm::style::{Attribute, Color, Stylize};

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

/// Stderr diagnostic severity for TTY styling. Failures draw red, benign
/// lifecycle and usage draw dim, plain streams stay unstyled. Gated by
/// `DisplayConfig::stderr_color` so piped stderr never carries escapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StderrKind {
    Plain,
    Dim,
    Failure,
}

fn paint_stderr(text: &str, display: &DisplayConfig, kind: StderrKind) -> String {
    if !display.stderr_color {
        return text.to_string();
    }
    match kind {
        StderrKind::Plain => text.to_string(),
        StderrKind::Dim => text.attribute(Attribute::Dim).to_string(),
        StderrKind::Failure => text.with(Color::Red).to_string(),
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
    /// Observed background shell sessions, in completion order.
    /// Observer-only: sessions stay server-owned.
    background_sessions: Vec<String>,
    /// Merged Markdown rules applied to display text only (surface
    /// `assistant`); the transcript keeps raw text so display never leaks
    /// into model context. Empty patterns are skipped by the helper.
    markdown_rules: Vec<wf_types::cli::CliMarkdownRule>,
    /// Merged tool summary templates for tool lines; without a match the
    /// builtin card applies so a bad contribution degrades gracefully.
    tool_templates: Vec<tui_core::tool_output::ToolSummaryTemplate>,
}

impl<S: TurnSink + Default> TurnRenderer<S> {
    pub(crate) fn new(display: DisplayConfig) -> Self {
        let mode = display.markdown;
        let color = display.color;
        Self {
            append: AppendWriter::new(),
            stream: StreamRenderer::new(mode, color),
            display,
            line_pending: false,
            assistant_text: String::new(),
            sink: S::default(),
            background_sessions: Vec::new(),
            markdown_rules: Vec::new(),
            tool_templates: Vec::new(),
        }
    }

    /// Install merged plugin display sources for this turn. Display-only;
    /// the transcript keeps raw text.
    pub(crate) fn set_plugin_display(
        &mut self,
        markdown_rules: Vec<wf_types::cli::CliMarkdownRule>,
        tool_templates: Vec<tui_core::tool_output::ToolSummaryTemplate>,
    ) {
        self.markdown_rules = markdown_rules;
        self.tool_templates = tool_templates;
    }

    pub(crate) fn display_text(&self, text: &str) -> String {
        if self.markdown_rules.is_empty() {
            return text.to_string();
        }
        tui_core::tool_output::apply_snapshot_markdown_rules(
            text,
            &self.markdown_rules,
            "assistant",
        )
    }

    /// Truncated error preview for a failed tool. The error leads when
    /// present, otherwise the raw result; empty sources stay empty so the
    /// failure line keeps its exact historical shape.
    pub(crate) fn error_preview(&self, result: &str, error: Option<&str>) -> String {
        let source = match error {
            Some(text) if !text.trim().is_empty() => text,
            _ if !result.trim().is_empty() => result,
            _ => return String::new(),
        };
        let config = &self.display.tool_output;
        tui_core::tool_output::collapse_output(
            source,
            config.error_preview_lines,
            config.preview_max_chars,
        )
        .text
    }

    /// Truncated success preview for high detail. Retrieval kinds never
    /// render bodies here; the caller renders the count line instead.
    pub(crate) fn success_preview(
        &self,
        tool_name: &str,
        result: &str,
        kind: wf_types::tool::DisplayKind,
    ) -> String {
        if result.trim().is_empty() {
            return String::new();
        }
        let config = &self.display.tool_output;
        if kind == wf_types::tool::DisplayKind::ShellTail {
            return tui_core::tool_output::collapse_shell_tail(
                result,
                config.shell_preview_lines,
                config.preview_max_chars,
            )
            .text;
        }
        let _ = tool_name;
        tui_core::tool_output::collapse_output(
            result,
            config.preview_max_lines,
            config.preview_max_chars,
        )
        .text
    }

    pub(crate) fn tool_start_line(&self, tool_name: &str) -> String {
        if let Some(rendered) = self
            .tool_templates
            .iter()
            .find(|template| template.tool_name == tool_name)
            .and_then(|template| template.render_call(""))
        {
            if rendered.trim().is_empty() {
                return format!("tool start: {tool_name}");
            }
            return format!("tool start: {tool_name} {rendered}");
        }
        format!("tool start: {tool_name}")
    }

    /// Take the accumulated assistant text for the transcript. Rendering
    /// state (stdout flushing) is untouched.
    pub(crate) fn assistant_text(&mut self) -> String {
        std::mem::take(&mut self.assistant_text)
    }

    pub(crate) fn on_event(&mut self, event: &ExecutionStreamEvent) -> Option<bool> {
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
                let shown = self.display_text(&clean);
                let ready = match self.display.markdown {
                    MarkdownMode::Raw => self.append.push(&shown),
                    MarkdownMode::Plain | MarkdownMode::Styled => self.stream.push(&shown),
                };
                if !ready.is_empty() {
                    self.sink.stdout_text(&ready);
                    self.line_pending = !ready.ends_with('\n');
                }
                None
            }
            ExecutionStreamEvent::ToolStart { tool_name, .. } => {
                if !self.display.effective_show_tools() {
                    return None;
                }
                if self.display.tool_output.level == tui_core::tool_output::DetailLevel::Low {
                    return None;
                }
                self.end_stdout_line();
                let line = paint_stderr(
                    &self.tool_start_line(tool_name),
                    &self.display,
                    StderrKind::Plain,
                );
                self.sink.stderr_line(&line);
                None
            }
            ExecutionStreamEvent::ToolEnd {
                tool_name,
                success,
                result,
                error,
                display,
                background,
                ..
            } => {
                if *success {
                    if let Some(marker) = background {
                        if marker.is_active() {
                            if !self.background_sessions.contains(&marker.session_id) {
                                if self.background_sessions.len()
                                    >= wf_types::tool::MAX_OBSERVED_BACKGROUND_TASKS
                                {
                                    self.background_sessions.remove(0);
                                }
                                self.background_sessions.push(marker.session_id.clone());
                            }
                            self.end_stdout_line();
                            let line = paint_stderr(
                                &format!(
                                    "background task: {tool_name} running as {} (follow up with shell_output or shell_wait)",
                                    marker.session_id
                                ),
                                &self.display,
                                StderrKind::Dim,
                            );
                            self.sink.stderr_line(&line);
                        }
                    }
                } else {
                    self.end_stdout_line();
                    let line = paint_stderr(
                        &format!("tool failed: {tool_name}"),
                        &self.display,
                        StderrKind::Failure,
                    );
                    self.sink.stderr_line(&line);
                    let preview = self.error_preview(result, error.as_deref());
                    if !preview.is_empty() {
                        self.sink.stderr_line(&paint_stderr(
                            &preview,
                            &self.display,
                            StderrKind::Failure,
                        ));
                    }
                    return None;
                }
                if !self.display.effective_show_tools() {
                    return None;
                }
                self.end_stdout_line();
                let mut summary = format!("tool ok: {tool_name}");
                let mut body = String::new();
                if self.display.tool_output.level == tui_core::tool_output::DetailLevel::High {
                    let kind = display
                        .as_ref()
                        .map(|payload| payload.kind)
                        .unwrap_or_else(|| tui_core::tool_output::infer_kind(tool_name));
                    if kind == wf_types::tool::DisplayKind::Count {
                        let total = result.lines().count();
                        if total == 1 {
                            summary.push_str(" · 1 line");
                        } else if total > 1 {
                            summary.push_str(&format!(" · {total} lines"));
                        }
                    } else {
                        body = self.success_preview(tool_name, result, kind);
                    }
                }
                let line = paint_stderr(&summary, &self.display, StderrKind::Dim);
                self.sink.stderr_line(&line);
                if !body.is_empty() {
                    self.sink
                        .stderr_line(&paint_stderr(&body, &self.display, StderrKind::Dim));
                }
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
                cache_read_tokens,
                ..
            } => {
                if !self.display.show_usage {
                    return None;
                }
                self.end_stdout_line();
                let mut raw = format!(
                    "usage: {prompt_tokens} prompt + {completion_tokens} completion tokens"
                );
                if let Some(cached) = cache_read_tokens {
                    raw.push_str(&format!(" ({cached} cached)"));
                }
                if let Some(value) = cost {
                    raw.push_str(&format!(" (~${value:.4}"));
                    raw.push(')');
                }
                self.sink
                    .stderr_line(&paint_stderr(&raw, &self.display, StderrKind::Dim));
                None
            }
            ExecutionStreamEvent::SubAgentStarted { name, .. } => {
                if !self.display.effective_show_tools() {
                    return None;
                }
                self.end_stdout_line();
                let line = paint_stderr(
                    &format!("subagent started: {name}"),
                    &self.display,
                    StderrKind::Dim,
                );
                self.sink.stderr_line(&line);
                None
            }
            ExecutionStreamEvent::SubAgentEnded { name, success, .. } => {
                if !*success {
                    self.end_stdout_line();
                    let line = paint_stderr(
                        &format!("subagent failed: {name}"),
                        &self.display,
                        StderrKind::Failure,
                    );
                    self.sink.stderr_line(&line);
                    return None;
                }
                if !self.display.effective_show_tools() {
                    return None;
                }
                self.end_stdout_line();
                let line = paint_stderr(
                    &format!("subagent done: {name}"),
                    &self.display,
                    StderrKind::Dim,
                );
                self.sink.stderr_line(&line);
                None
            }
            ExecutionStreamEvent::SubAgentProgress { activity, .. } => {
                let text = activity.as_deref().filter(|t| !t.trim().is_empty())?;
                if !self.display.effective_show_tools() {
                    return None;
                }
                self.end_stdout_line();
                let line =
                    paint_stderr(&format!("subagent: {text}"), &self.display, StderrKind::Dim);
                self.sink.stderr_line(&line);
                None
            }
            ExecutionStreamEvent::Completed { iterations, .. } => {
                self.flush_stdout();
                if !self.background_sessions.is_empty() {
                    let line = paint_stderr(
                        &format!(
                            "background tasks: {} running ({}) — follow up with shell_output or shell_wait, confirm with shell_ack",
                            self.background_sessions.len(),
                            self.background_sessions.join(", "),
                        ),
                        &self.display,
                        StderrKind::Dim,
                    );
                    self.sink.stderr_line(&line);
                }
                if self.display.effective_show_tools() {
                    let line = paint_stderr(
                        &format!("completed in {iterations} iterations"),
                        &self.display,
                        StderrKind::Dim,
                    );
                    self.sink.stderr_line(&line);
                }
                Some(true)
            }
            ExecutionStreamEvent::Failed { error } => {
                self.flush_stdout();
                let line = paint_stderr(
                    &format!("failed: {}", strip_terminal_escapes(error)),
                    &self.display,
                    StderrKind::Failure,
                );
                self.sink.stderr_line(&line);
                Some(false)
            }
            ExecutionStreamEvent::Interrupted { reason } => {
                self.flush_stdout();
                let line = paint_stderr(
                    &format!("interrupted: {}", strip_terminal_escapes(reason)),
                    &self.display,
                    StderrKind::Failure,
                );
                self.sink.stderr_line(&line);
                Some(false)
            }
        }
    }

    pub(crate) fn flush_stdout(&mut self) {
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

    pub(crate) fn end_stdout_line(&mut self) {
        if self.line_pending {
            self.sink.stdout_text("\n");
            self.line_pending = false;
        }
    }

    pub(crate) fn finish(&mut self) {
        self.flush_stdout();
        let _ = std::io::Write::flush(&mut std::io::stderr());
    }
}

#[cfg(test)]
mod test;
