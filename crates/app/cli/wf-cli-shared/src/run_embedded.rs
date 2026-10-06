//! Embedded headless session driver (`run_session`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::approval::StdioApprovalHandler;
use crate::domain::DomainAdapter;
use crate::error::{CliError, CliResult};
use crate::output::{OutputFormat, OutputMessage};
use crate::run::{RunIo, RunOptions, RunOutcome};
use crate::run_interaction::HeadlessInteractionGuard;
use crate::run_render::SessionRenderer;
use crate::run_stream::{drive_embedded_stream, finish_workflow_stream_terminal, Terminal};
use crate::run_summary::{
    had_output_for_result, render_value_text, write_failure_envelope, write_summary, SummaryParams,
};
use crate::stdio_prompt::StdioPromptSource;
use wf_api::agent::agent_execution;
use wf_api::entity::user_interaction::{register_handler, UserInteractionHandler};
use wf_runtime::tool_approval::{ApprovalPolicy, PolicyApprovalHandler};

/// Drive one headless agent session to its terminal event.
///
/// Returns the outcome on success; business failures map to
/// [`CliError::Business`] (exit 1) and interruptions (SIGINT or an engine
/// `Interrupted` event) to [`CliError::Interrupted`] (exit 4).
pub async fn run_session(
    adapter: &DomainAdapter,
    opts: RunOptions,
    mut io: RunIo,
) -> CliResult<RunOutcome> {
    // Workflow foreground path: stream engine events through the same
    // renderer as agent turns so tool/output progress is visible live.
    // Falls back to blocking `execute` only if the stream cannot start.
    if let Some(workflow_id) = opts.workflow.clone() {
        let input = crate::turn::parse_workflow_input(opts.workflow_input.as_deref())
            .map_err(CliError::Arguments)?;
        let started = Instant::now();
        if !io.format.is_silent() {
            io.sink.write_message(&OutputMessage::new(
                "user",
                format!("workflow:{workflow_id}"),
            ))?;
        }
        let ctx = adapter.api_context();
        // Prefer streaming so tool/output progress renders live through the
        // shared renderer; fall back to blocking `execute` if needed.
        let stream_attempt = crate::turn::stream_workflow_turn(
            adapter.api_context_arc(),
            &workflow_id,
            input.clone(),
        )
        .await;
        if let Ok((stream_id, mut stream)) = stream_attempt {
            let execution_id = stream_id.to_string();
            let mut renderer = SessionRenderer::new(io.sink.as_mut(), io.format);
            let terminal = drive_embedded_stream(&mut stream, &mut renderer, &io.diag).await?;
            drop(stream);
            renderer.finish()?;
            let had_output = renderer.had_output;
            drop(renderer);
            io.sink.flush()?;
            return finish_workflow_stream_terminal(
                terminal,
                &mut io,
                &opts,
                &execution_id,
                started,
                had_output,
            );
        }
        let params = wf_api::workflow::workflow_execution::ExecuteWorkflowParams {
            workflow_id: workflow_id.clone(),
            input,
            options: None,
        };
        let output = wf_api::workflow::workflow_execution::execute(ctx, params)
            .await
            .map_err(CliError::from)?;
        let execution_id = output.execution_id.to_string();
        let result = output.result.clone();
        let had_output = had_output_for_result(&result);
        if !io.format.is_silent() && io.format == OutputFormat::Text {
            render_value_text(io.sink.as_mut(), &result)?;
        }
        io.sink.flush()?;
        let duration_ms = started.elapsed().as_millis() as u64;
        write_summary(SummaryParams {
            sink: io.sink.as_mut(),
            format: io.format,
            execution_id: &execution_id,
            iterations: 1u32,
            duration_ms,
            had_output,
            opts: &opts,
            result: Some(&result),
        })?;
        io.sink.flush()?;
        return Ok(RunOutcome {
            execution_id,
            iterations: 1,
            duration_ms,
            had_output,
        });
    }

    if opts.prompt.trim().is_empty() {
        return Err(CliError::Arguments(
            "no prompt given: pass a positional argument or pipe stdin".into(),
        ));
    }

    let started = Instant::now();
    let execution_id = wf_common::generate_id();
    let ctx = adapter.api_context();

    // Follow-up delivery: flag-only by default (fail the run afterwards),
    // stdin-answered with `--interactive`. The prompt source is shared
    // between the approval handler and the interaction guard so answer
    // lines stay in arrival order.
    let followup_requested = Arc::new(AtomicBool::new(false));
    let prompt_source = if opts.interactive {
        Some(StdioPromptSource::spawn().0)
    } else {
        None
    };
    let approval_timeout = Duration::from_secs(opts.approval_timeout_secs);
    let json_protocol = matches!(io.format, OutputFormat::Json | OutputFormat::JsonLines);
    let guard: Arc<dyn UserInteractionHandler> = match &prompt_source {
        Some(source) => Arc::new(HeadlessInteractionGuard::interactive(
            followup_requested.clone(),
            io.diag.clone(),
            source.clone(),
            adapter.api_context_arc(),
            approval_timeout,
            json_protocol,
        )),
        None => Arc::new(HeadlessInteractionGuard::flag_only(
            followup_requested.clone(),
            io.diag.clone(),
        )),
    };
    register_handler(ctx, guard).await;

    // Policy-first wiring shared with the server: the engine evaluates the
    // baseline policy (denials terminal), the approval handler answers only
    // `Ask` decisions. `--interactive` prompts on stdin for the rest,
    // `--assume-yes` approves them blindly, otherwise the prefix whitelist
    // decides without interaction. Config assembly lives in
    // `turn::build_agent_loop_params` (single source with mini/TUI).
    let turn_params = opts.as_turn_params();
    let diag_reporter = io.diag.clone();
    let reporter: wf_runtime::tool_approval::DecisionReporter = Arc::new(move |report| {
        let mut diag = wf_common::lock::lock_ok(diag_reporter.lock());
        if report.allowed {
            let _ = diag.ok(&format!("▲ {} ({})", report.tool_name, report.reason));
        } else {
            let _ = diag.err(&format!("✗ {}: {}", report.tool_name, report.reason));
        }
    });
    let approval_handler: Arc<dyn wf_api::ToolApprovalHandler> =
        match (&prompt_source, opts.assume_yes) {
            (Some(source), false) => Arc::new(StdioApprovalHandler::interactive(
                ApprovalPolicy::new(opts.approve_prefixes.clone()),
                source.clone(),
                io.diag.clone(),
                approval_timeout,
                json_protocol,
            )),
            (_, true) => Arc::new(StdioApprovalHandler::assume_yes(io.diag.clone())),
            (None, false) => Arc::new(
                PolicyApprovalHandler::new(ApprovalPolicy::new(opts.approve_prefixes.clone()))
                    .with_reporter(reporter),
            ),
        };
    let mut params = crate::turn::build_agent_loop_params(
        &turn_params,
        Some(wf_runtime::tool_approval::headless_approval_options(Some(
            ctx,
        ))),
        Some(approval_handler),
    );
    params.agent_loop_id = Some(wf_types::Id::from(execution_id.clone()));
    // Composition boundary: resolve the agent template before execution so
    // headless runs share the TUI/server defaults.
    let env = wf_execution_shared::agent_prompt::PromptEnvironment::new(
        Some(ctx.registries.as_ref()),
        Some(ctx.tool_registry.as_ref()),
        ctx.metrics.as_deref(),
    );
    let params =
        wf_api::agent::composition::resolve_run_params(&env, params).map_err(CliError::from)?;

    // Echo the user message through the sink (text line / JSON record).
    if !io.format.is_silent() {
        io.sink
            .write_message(&OutputMessage::new("user", &opts.prompt))?;
    }

    let mut stream = agent_execution::stream(ctx, params)
        .await
        .map_err(CliError::from)?;

    let mut renderer = SessionRenderer::new(io.sink.as_mut(), io.format);
    let terminal = drive_embedded_stream(&mut stream, &mut renderer, &io.diag).await?;
    // Dropping the stream aborts the agent driver task chain.
    drop(stream);

    renderer.finish()?;
    // End the renderer's sink borrow before the terminal arms reuse it.
    let had_output = renderer.had_output;
    drop(renderer);
    io.sink.flush()?;

    match terminal {
        Terminal::Completed { iterations, .. } => {
            if followup_requested.load(Ordering::SeqCst) {
                return Err(CliError::Business(
                    "follow-up question requested in headless mode; \
                     re-run interactively (wf --tui) to answer it"
                        .into(),
                ));
            }
            let duration_ms = started.elapsed().as_millis() as u64;
            write_summary(SummaryParams {
                sink: io.sink.as_mut(),
                format: io.format,
                execution_id: &execution_id,
                iterations,
                duration_ms,
                had_output,
                opts: &opts,
                result: None,
            })?;
            io.sink.flush()?;
            Ok(RunOutcome {
                execution_id,
                iterations,
                duration_ms,
                had_output,
            })
        }
        Terminal::Failed { error } => {
            write_failure_envelope(io.sink.as_mut(), io.format, &execution_id, &error);
            let _ = io.sink.flush();
            Err(CliError::Business(error))
        }
        Terminal::Interrupted { reason } => Err(CliError::Interrupted(reason)),
        Terminal::Sigint => {
            let mut diag = wf_common::lock::lock_ok(io.diag.lock());
            let _ = diag.warn("^C interrupted");
            Err(CliError::Interrupted(
                "SIGINT during headless session".into(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    mod e2e {
        use super::super::run_session;
        use crate::domain::DomainAdapter;
        use crate::error::CliError;
        use crate::output::{MemorySink, OutputFormat, OutputMessage, OutputSink};
        use crate::run::{RunIo, RunOptions};
        use crate::run_diag::DiagWriter;
        use std::sync::{Arc, Mutex};
        use wf_llm::{LlmResponseSpec, MockLlmClient};

        fn run_io(format: OutputFormat) -> (RunIo, Arc<Mutex<MemorySink>>) {
            let sink = Arc::new(Mutex::new(MemorySink::new()));
            // `run_session` needs an owned sink; the Arc handle keeps read
            // access for assertions while the session owns the writer half
            // through a shared-memory forwarder.
            let forwarding = SinkForwarder::new(sink.clone());
            let io = RunIo {
                sink: Box::new(forwarding),
                diag: Arc::new(Mutex::new(DiagWriter::buffer())),
                format,
            };
            (io, sink)
        }

        /// Forwards writes into a shared `MemorySink` so tests can read the
        /// accumulated output while the session driver owns the sink.
        struct SinkForwarder {
            shared: Arc<Mutex<MemorySink>>,
        }

        impl SinkForwarder {
            fn new(shared: Arc<Mutex<MemorySink>>) -> Self {
                Self { shared }
            }

            fn with_sink<R>(&self, f: impl FnOnce(&mut MemorySink) -> R) -> R {
                f(&mut wf_common::lock::lock_ok(self.shared.lock()))
            }
        }

        impl OutputSink for SinkForwarder {
            fn write_message(&mut self, message: &OutputMessage) -> std::io::Result<()> {
                self.with_sink(|sink| sink.write_message(message))
            }
            fn write_chunk(&mut self, chunk: &str) -> std::io::Result<()> {
                self.with_sink(|sink| sink.write_chunk(chunk))
            }
            fn write_raw(&mut self, line: &str) -> std::io::Result<()> {
                self.with_sink(|sink| sink.write_raw(line))
            }
            fn write_text(&mut self, text: &str) -> std::io::Result<()> {
                self.with_sink(|sink| sink.write_text(text))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                self.with_sink(|sink| sink.flush())
            }
        }

        async fn adapter_with_mock(
            script: Vec<LlmResponseSpec>,
            default: LlmResponseSpec,
        ) -> DomainAdapter {
            let adapter = DomainAdapter::bootstrap(crate::default_runtime_config())
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
        async fn headless_session_streams_mock_llm_answer_end_to_end() {
            let adapter = adapter_with_mock(
                vec![LlmResponseSpec::text("hello from cli e2e")],
                LlmResponseSpec::text("fallback"),
            )
            .await;
            let (io, sink) = run_io(OutputFormat::Text);

            let outcome = run_session(
                &adapter,
                RunOptions {
                    prompt: "hi".into(),
                    model: Some("mock".into()),
                    ..Default::default()
                },
                io,
            )
            .await
            .unwrap();

            assert_eq!(outcome.iterations, 1);
            assert!(outcome.had_output);
            {
                let sink_guard = wf_common::lock::lock_ok(sink.lock());
                let text = sink_guard.text();
                assert!(text.contains("hello from cli e2e"), "{text}");
                // The summary line is a raw record (bypasses the format filter).
                let summary = sink_guard
                    .raw()
                    .into_iter()
                    .find(|line| line.contains('▣'))
                    .unwrap_or_else(|| panic!("summary line missing from {text:?}"));
                assert!(
                    summary.contains(&outcome.execution_id),
                    "summary {summary} lacks execution id"
                );
            }

            adapter.shutdown().await.unwrap();
        }

        #[tokio::test]
        async fn headless_session_jsonl_emits_summary_record() {
            let adapter = adapter_with_mock(
                vec![LlmResponseSpec::text("jsonl answer")],
                LlmResponseSpec::text("fallback"),
            )
            .await;
            let (io, sink) = run_io(OutputFormat::JsonLines);

            let outcome = run_session(
                &adapter,
                RunOptions {
                    prompt: "hi".into(),
                    model: Some("mock".into()),
                    ..Default::default()
                },
                io,
            )
            .await
            .unwrap();

            let raw: Vec<String> = wf_common::lock::lock_ok(sink.lock())
                .raw()
                .into_iter()
                .map(str::to_string)
                .collect();
            let summary = raw
                .iter()
                .find(|line| line.contains("execution_summary"))
                .unwrap_or_else(|| panic!("no summary record in {raw:?}"));
            let parsed: serde_json::Value = serde_json::from_str(summary).unwrap();
            assert_eq!(parsed["success"], true);
            assert_eq!(parsed["executionId"], outcome.execution_id);
            assert_eq!(parsed["iterations"], 1);

            adapter.shutdown().await.unwrap();
        }

        #[tokio::test]
        async fn sensitive_tool_call_is_denied_and_session_recovers() {
            // `execute_command` is discoverable under `@standard/main`, so a
            // direct call is rejected by the exposure gate before approval.
            // Route through the `general` proxy: the outer shell is visible,
            // the inner call is allowed by exposure and then hits the headless
            // policy, which denies it with the sensitive-tool reason on diag.
            let tool_call = wf_types::message::LlmToolCall {
                id: "call-1".into(),
                r#type: "function".into(),
                function: wf_types::message::LlmFunctionCall {
                    name: "general".into(),
                    arguments: serde_json::json!({
                        "request": serde_json::json!({
                            "tool": "execute_command",
                            "parameters": { "command": "rm -rf /" },
                        })
                        .to_string(),
                    })
                    .to_string(),
                },
            };
            let adapter = adapter_with_mock(
                vec![LlmResponseSpec::tool_calls(vec![tool_call])],
                LlmResponseSpec::text("gave up on the tool"),
            )
            .await;
            let (io, sink) = run_io(OutputFormat::Text);
            let diag = io.diag.clone();

            let outcome = run_session(
                &adapter,
                RunOptions {
                    prompt: "clean my disk".into(),
                    model: Some("mock".into()),
                    ..Default::default()
                },
                io,
            )
            .await
            .unwrap();

            // The denial is visible on the diagnostics channel, the run
            // itself completes with the follow-up text answer.
            let diag_text = wf_common::lock::lock_ok(diag.lock()).snapshot();
            assert!(diag_text.contains("✗"), "{diag_text}");
            assert!(diag_text.contains("execute_command"), "{diag_text}");
            assert!(diag_text.contains("sensitive"), "{diag_text}");
            assert!(wf_common::lock::lock_ok(sink.lock())
                .text()
                .contains("gave up on the tool"));

            // The rejected call is fed back: the LLM saw a second request.
            assert!(outcome.iterations >= 1);

            adapter.shutdown().await.unwrap();
        }

        #[tokio::test]
        async fn empty_prompt_fails_fast_with_arguments_error() {
            let adapter = DomainAdapter::bootstrap(crate::default_runtime_config())
                .await
                .unwrap();
            let (io, _sink) = run_io(OutputFormat::Text);

            let err = run_session(&adapter, RunOptions::default(), io)
                .await
                .unwrap_err();
            assert!(matches!(err, CliError::Arguments(_)), "{err:?}");

            adapter.shutdown().await.unwrap();
        }

        #[tokio::test]
        async fn low_risk_tool_is_allowed_and_executes() {
            let file = tempfile::NamedTempFile::new().unwrap();
            std::fs::write(file.path(), "line1\nline2\n").unwrap();
            let tool_call = wf_types::message::LlmToolCall {
                id: "call-read".into(),
                r#type: "function".into(),
                function: wf_types::message::LlmFunctionCall {
                    name: "read_file".into(),
                    arguments: serde_json::json!({ "path": file.path().to_string_lossy() })
                        .to_string(),
                },
            };
            let adapter = adapter_with_mock(
                vec![LlmResponseSpec::tool_calls(vec![tool_call])],
                LlmResponseSpec::text("read it"),
            )
            .await;
            let (io, _sink) = run_io(OutputFormat::Text);
            let diag = io.diag.clone();

            let outcome = run_session(
                &adapter,
                RunOptions {
                    prompt: "read the file".into(),
                    model: Some("mock".into()),
                    ..Default::default()
                },
                io,
            )
            .await
            .unwrap();

            // read_file is ReadOnly: the engine policy auto-approves it
            // without consulting the handler (no allow-list diag line), the
            // tool actually executes (▲ start / ✓ done lines) and the
            // session completes normally.
            let diag_text = wf_common::lock::lock_ok(diag.lock()).snapshot();
            assert!(diag_text.contains("▲ read_file"), "{diag_text}");
            assert!(diag_text.contains("✓ read_file"), "{diag_text}");
            assert!(!diag_text.contains("✗ read_file"), "{diag_text}");
            assert!(
                !diag_text.contains("allow-listed"),
                "policy auto-approval must not consult the handler: {diag_text}"
            );
            assert_eq!(outcome.iterations, 2, "tool turn + final answer turn");

            adapter.shutdown().await.unwrap();
        }

        #[tokio::test]
        async fn llm_error_maps_to_business_failure() {
            let adapter = DomainAdapter::bootstrap(crate::default_runtime_config())
                .await
                .unwrap();
            let mock = Arc::new(MockLlmClient::new());
            // Every attempt (including gateway retries) errors: the script
            // queue is consumed per request, so saturate it.
            for _ in 0..16 {
                mock.script_error(wf_llm::LlmError::ProviderError {
                    status: None,
                    message: "provider exploded".into(),
                });
            }
            adapter.llm_gateway().register_mock("mock", mock);
            let (io, _sink) = run_io(OutputFormat::Text);

            let err = run_session(
                &adapter,
                RunOptions {
                    prompt: "hi".into(),
                    model: Some("mock".into()),
                    ..Default::default()
                },
                io,
            )
            .await
            .unwrap_err();

            match err {
                CliError::Business(ref msg) => assert!(msg.contains("provider exploded"), "{msg}"),
                other => panic!("expected business failure, got {other:?}"),
            }
            assert_eq!(err.exit_code(), 1);

            adapter.shutdown().await.unwrap();
        }

        #[tokio::test]
        async fn silent_session_reports_no_output_in_summary() {
            let adapter =
                adapter_with_mock(vec![LlmResponseSpec::text("")], LlmResponseSpec::text("")).await;
            let (io, sink) = run_io(OutputFormat::Text);

            let outcome = run_session(
                &adapter,
                RunOptions {
                    prompt: "hi".into(),
                    model: Some("mock".into()),
                    ..Default::default()
                },
                io,
            )
            .await
            .unwrap();

            assert!(!outcome.had_output);
            let raw_lines: Vec<String> = wf_common::lock::lock_ok(sink.lock())
                .raw()
                .into_iter()
                .map(str::to_string)
                .collect();
            let summary = raw_lines
                .iter()
                .find(|line| line.contains('▣'))
                .unwrap_or_else(|| panic!("summary line missing"));
            assert!(
                summary.contains("no output"),
                "summary should report no output: {summary}"
            );

            adapter.shutdown().await.unwrap();
        }

        #[tokio::test]
        async fn headless_workflow_executes_to_completed() {
            let adapter = DomainAdapter::bootstrap(crate::default_runtime_config())
                .await
                .unwrap();
            // Minimal workflow: start -> variable -> end
            let definition = wf_types::workflow::WorkflowDefinition {
                id: "wf-cli-workflow-test".into(),
                name: "Workflow Test".into(),
                description: None,
                r#type: None,
                version: Some("1.0.0".into()),
                nodes: vec![
                    wf_types::node::BaseStaticNode {
                        id: "start".into(),
                        node_type: wf_types::node::StaticNodeType::Start,
                        name: Some("start".into()),
                        description: None,
                        config: None,
                        execution_config: None,
                    },
                    wf_types::node::BaseStaticNode {
                        id: "v1".into(),
                        node_type: wf_types::node::StaticNodeType::Variable,
                        name: Some("v1".into()),
                        description: None,
                        config: Some(serde_json::json!({
                            "variable_name": "final",
                            "expression": "${input.greeting}",
                        })),
                        execution_config: None,
                    },
                    wf_types::node::BaseStaticNode {
                        id: "end".into(),
                        node_type: wf_types::node::StaticNodeType::End,
                        name: Some("end".into()),
                        description: None,
                        config: None,
                        execution_config: None,
                    },
                ],
                edges: vec![
                    wf_types::workflow::Edge {
                        id: "e1".into(),
                        source_node_id: "start".into(),
                        target_node_id: "v1".into(),
                        r#type: wf_types::workflow::edge::EdgeType::Default,
                        condition: None,
                        label: None,
                        description: None,
                        weight: None,
                        metadata: None,
                        error_route: None,
                    },
                    wf_types::workflow::Edge {
                        id: "e2".into(),
                        source_node_id: "v1".into(),
                        target_node_id: "end".into(),
                        r#type: wf_types::workflow::edge::EdgeType::Default,
                        condition: None,
                        label: None,
                        description: None,
                        weight: None,
                        metadata: None,
                        error_route: None,
                    },
                ],
                config: None,
                variables: None,
                triggered_subworkflow_config: None,
                metadata: None,
                available_tools: None,
                hooks: None,
                created_at: wf_common::now(),
                updated_at: wf_common::now(),
            };
            wf_api::workflow::save_workflow(adapter.api_context(), &definition)
                .await
                .unwrap();

            let (io, sink) = run_io(OutputFormat::Text);
            let outcome = run_session(
                &adapter,
                RunOptions {
                    prompt: String::new(),
                    workflow: Some("wf-cli-workflow-test".into()),
                    workflow_input: Some(r#"{"greeting":"hello"}"#.into()),
                    ..Default::default()
                },
                io,
            )
            .await
            .unwrap();

            assert!(!outcome.execution_id.is_empty());
            assert!(outcome.had_output);
            let text = wf_common::lock::lock_ok(sink.lock()).text();
            // Workflow result should surface in text output (pretty JSON)
            assert!(
                text.contains("hello") || text.contains("greeting"),
                "{text}"
            );
            {
                let guard = wf_common::lock::lock_ok(sink.lock());
                let raw = guard.raw();
                assert!(raw.iter().any(|l| l.contains('▣')), "summary missing");
            }

            adapter.shutdown().await.unwrap();
        }

        #[tokio::test]
        async fn headless_workflow_invalid_input_is_rejected() {
            let adapter = DomainAdapter::bootstrap(crate::default_runtime_config())
                .await
                .unwrap();
            let (io, _sink) = run_io(OutputFormat::Text);
            let err = run_session(
                &adapter,
                RunOptions {
                    prompt: String::new(),
                    workflow: Some("missing-wf".into()),
                    workflow_input: Some("bad-json".into()),
                    ..Default::default()
                },
                io,
            )
            .await
            .unwrap_err();
            assert!(
                matches!(err, CliError::Arguments(_))
                    || matches!(err, CliError::Business(_))
                    || matches!(err, CliError::Api(_))
            );
            adapter.shutdown().await.unwrap();
        }
    }
}
