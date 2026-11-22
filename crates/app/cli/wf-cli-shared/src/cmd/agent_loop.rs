//! Detached agent-loop commands: `agent-loop dispatch` / `agent-loop attach`.
//!
//! Both talk to the keepalive HTTP endpoints. `dispatch` submits a run in the
//! background and returns its execution id; `attach` replays retained history
//! then follows live events until the synthetic terminal marker.

use futures::StreamExt;

use crate::args::{AgentLoopSub, Cli};
use crate::cmd::render::render_envelope_to_cli;
use crate::connection::ClientHandle;
use crate::error::CliResult;
use crate::output::OutputEnvelope;
use crate::turn::{TurnKind, TurnParams};
use wf_client::WfClient;

pub async fn run(cli: &Cli, sub: &AgentLoopSub) -> CliResult<()> {
    let handle = ClientHandle::from_cli(cli).await?;
    let client = handle.client();
    match sub {
        AgentLoopSub::Dispatch {
            prompt,
            loop_id,
            agent,
            model,
            context,
            timeout,
        } => dispatch(cli, client, prompt, loop_id, agent, model, context, *timeout).await,
        AgentLoopSub::Attach {
            execution_id,
            since,
        } => attach(cli, client, execution_id, since.as_deref()).await,
    }
}

/// Build the same wire body the regular agent stream uses, then POST it to
/// the dispatch endpoint. The server pre-registers an execution id and keeps
/// the run alive independent of this connection.
#[allow(clippy::too_many_arguments)]
async fn dispatch(
    cli: &Cli,
    client: &WfClient,
    prompt: &str,
    loop_id: &str,
    agent: &Option<String>,
    model: &Option<String>,
    context: &Option<String>,
    timeout_ms: Option<u64>,
) -> CliResult<()> {
    let context = match context {
        Some(raw) => serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(raw)
            .map_err(|e| crate::error::CliError::Arguments(format!("invalid --context JSON: {e}")))?,
        None => serde_json::Map::new(),
    };
    let params = TurnParams {
        agent: agent.clone(),
        model: model.clone(),
        approve_prefixes: Vec::new(),
        timeout_ms,
        context,
        conversation: Vec::new(),
        kind: TurnKind::Agent {
            prompt: prompt.to_string(),
            images: Vec::new(),
            attachments: Vec::new(),
        },
    };
    let body = params.agent_request_body();
    let view: serde_json::Value = client
        .post_json(&wf_client::endpoints::agent_loop_dispatch(loop_id), &body)
        .await?;
    render_envelope_to_cli(
        cli,
        OutputEnvelope::success("agent-loop-dispatch", view)
            .with_entity(loop_id.to_string()),
    )
}

/// Attach to a detached run: replay history + follow live events. Each frame
/// is emitted as its own envelope line (JSON) or a compact text line; the
/// synthetic `execution.terminal` marker ends the stream.
async fn attach(
    cli: &Cli,
    client: &WfClient,
    execution_id: &str,
    since: Option<&str>,
) -> CliResult<()> {
    let path = wf_client::endpoints::agent_loop_attach(execution_id, since);
    let mut frames = client.get_stream(&path).await?;
    while let Some(frame) = frames.next().await {
        let frame = frame?;
        // The server appends a synthetic terminal marker as a data frame with
        // `type: execution.terminal`; stop once we see it.
        let is_terminal = frame
            .data
            .get("type")
            .and_then(|v| v.as_str())
            .is_some_and(|t| t == "execution.terminal");
        let entity = execution_id.to_string();
        render_envelope_to_cli(
            cli,
            OutputEnvelope::success("agent-loop-event", frame.data).with_entity(entity),
        )?;
        if is_terminal {
            break;
        }
    }
    Ok(())
}
