use wf_client::WfClient;

use crate::args::{AnalysisSub, Cli};
use crate::cmd::render::render_envelope;
use crate::connection::ClientHandle;
use crate::error::CliResult;
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &AnalysisSub) -> CliResult<()> {
    let handle = ClientHandle::from_cli(cli).await?;
    let client = handle.client();
    match sub {
        AnalysisSub::Performance { id } => {
            let data = performance_data(client, id).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("analysis-performance", data).with_entity(id.clone()),
            )
        }
        AnalysisSub::Bottleneck { id } => {
            let data = bottleneck_data(client, id).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("analysis-bottleneck", data).with_entity(id.clone()),
            )
        }
        AnalysisSub::Errors {
            id,
            chain,
            root_cause,
            recovery,
        } => {
            let data = errors_data(client, id, *chain, *root_cause, *recovery).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("analysis-errors", data).with_entity(id.clone()),
            )
        }
        AnalysisSub::Compare { baseline, compared } => {
            let data = compare_data(client, baseline, compared).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("analysis-compare", data)
                    .with_entity(format!("{baseline}:{compared}")),
            )
        }
        AnalysisSub::Progress { id } => {
            let data = progress_data(client, id).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("analysis-progress", data).with_entity(id.clone()),
            )
        }
    }
}

/// Shared analysis data builders: single source for the `analysis` command
/// and the `execution` convenience arms (`performance` / `bottleneck` /
/// `errors` / `compare` / `progress`). Callers wrap the value in their own
/// envelope so established command names stay stable.
pub async fn performance_data(client: &WfClient, id: &str) -> CliResult<serde_json::Value> {
    client.get_json(&format!("/api/v1/executions/{id}/performance")).await.map_err(crate::error::CliError::from)
}

/// Bottleneck list for one execution.
pub async fn bottleneck_data(client: &WfClient, id: &str) -> CliResult<serde_json::Value> {
    client
        .get_json(&format!("/api/v1/executions/{id}/performance/bottlenecks"))
        .await.map_err(crate::error::CliError::from)
}

/// Error diagnostic object: always includes stats, plus the requested
/// chain / root-cause / recovery sections (or the advanced rollup when
/// none is requested).
pub async fn errors_data(
    client: &WfClient,
    id: &str,
    chain: bool,
    root_cause: bool,
    recovery: bool,
) -> CliResult<serde_json::Value> {
    let mut out = serde_json::Map::new();
    let stats: serde_json::Value = client
        .get_json(&format!("/api/v1/executions/{id}/error-analysis"))
        .await?;
    out.insert("stats".into(), stats);
    if chain {
        let chain_data: serde_json::Value = client
            .get_json(&format!("/api/v1/executions/{id}/error-analysis/context"))
            .await?;
        out.insert("chain".into(), chain_data);
    }
    if root_cause {
        let rc: serde_json::Value = client
            .get_json(&format!("/api/v1/executions/{id}/error-analysis/root-cause"))
            .await?;
        out.insert("rootCause".into(), rc);
    }
    if recovery {
        let recs: serde_json::Value = client
            .get_json(&format!(
                "/api/v1/executions/{id}/error-analysis/recovery-recommendations"
            ))
            .await?;
        out.insert("recovery".into(), recs);
    }
    if !chain && !root_cause && !recovery {
        let advanced: serde_json::Value = client
            .get_json(&format!("/api/v1/executions/{id}/error-analysis/advanced"))
            .await?;
        out.insert("advanced".into(), advanced);
    }
    Ok(serde_json::Value::Object(out))
}

/// Baseline-vs-compared execution comparison.
pub async fn compare_data(
    client: &WfClient,
    baseline: &str,
    compared: &str,
) -> CliResult<serde_json::Value> {
    client
        .get_json(&format!(
            "/api/v1/analysis/performance/compare?baseline={baseline}&compared={compared}"
        ))
        .await.map_err(crate::error::CliError::from)
}

/// Progress metrics for one execution.
pub async fn progress_data(client: &WfClient, id: &str) -> CliResult<serde_json::Value> {
    client.get_json(&format!("/api/v1/executions/{id}/progress")).await.map_err(crate::error::CliError::from)
}
