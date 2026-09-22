use wf_api::analysis::{error_analysis, performance, progress};
use wf_api::infra::context::ApiContext;

use crate::args::{AnalysisSub, Cli};
use crate::cmd::render::render_envelope;
use crate::error::CliResult;
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &AnalysisSub) -> CliResult<()> {
    let domain =
        crate::domain::DomainHandle::require_embedded(cli, crate::mode::CliMode::Run, "analysis")
            .await?;
    let ctx = domain
        .api_context()
        .expect("embedded mode must have api_context");
    let result = match sub {
        AnalysisSub::Performance { id } => {
            let data = performance_data(ctx, id).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("analysis-performance", data).with_entity(id.clone()),
            )
        }
        AnalysisSub::Bottleneck { id } => {
            let data = bottleneck_data(ctx, id).await?;
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
            let data = errors_data(ctx, id, *chain, *root_cause, *recovery).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("analysis-errors", data).with_entity(id.clone()),
            )
        }
        AnalysisSub::Compare { baseline, compared } => {
            let data = compare_data(ctx, baseline, compared).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("analysis-compare", data)
                    .with_entity(format!("{baseline}:{compared}")),
            )
        }
        AnalysisSub::Progress { id } => {
            let data = progress_data(ctx, id).await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("analysis-progress", data).with_entity(id.clone()),
            )
        }
    };
    domain.shutdown().await?;
    result
}

/// Shared analysis data builders: single source for the `analysis` command
/// and the `execution` convenience arms (`performance` / `bottleneck` /
/// `errors` / `compare` / `progress`). Callers wrap the value in their own
/// envelope so established command names stay stable.
pub async fn performance_data(ctx: &ApiContext, id: &str) -> CliResult<serde_json::Value> {
    let profile = performance::analyze_performance(ctx, id).await?;
    Ok(serde_json::to_value(&profile)?)
}

/// Bottleneck list for one execution.
pub async fn bottleneck_data(ctx: &ApiContext, id: &str) -> CliResult<serde_json::Value> {
    let bottlenecks = performance::identify_bottlenecks(ctx, id).await?;
    Ok(serde_json::to_value(&bottlenecks)?)
}

/// Error diagnostic object: always includes stats, plus the requested
/// chain / root-cause / recovery sections (or the advanced rollup when
/// none is requested).
pub async fn errors_data(
    ctx: &ApiContext,
    id: &str,
    chain: bool,
    root_cause: bool,
    recovery: bool,
) -> CliResult<serde_json::Value> {
    let mut out = serde_json::Map::new();
    let stats = error_analysis::workflow_error_stats(ctx, id).await?;
    out.insert(
        "stats".into(),
        serde_json::to_value(&stats).unwrap_or(serde_json::Value::Null),
    );
    if chain {
        let chain_data = error_analysis::get_error_chain(ctx, id, None).await?;
        out.insert(
            "chain".into(),
            serde_json::to_value(&chain_data).unwrap_or(serde_json::Value::Null),
        );
    }
    if root_cause {
        let rc = error_analysis::analyze_root_cause(ctx, id).await?;
        out.insert(
            "rootCause".into(),
            serde_json::to_value(&rc).unwrap_or(serde_json::Value::Null),
        );
    }
    if recovery {
        let recs = error_analysis::recovery_recommendations(ctx, id).await?;
        out.insert(
            "recovery".into(),
            serde_json::to_value(&recs).unwrap_or(serde_json::Value::Null),
        );
    }
    if !chain && !root_cause && !recovery {
        let advanced = error_analysis::get_advanced_error_analysis(ctx, id).await?;
        out.insert(
            "advanced".into(),
            serde_json::to_value(&advanced).unwrap_or(serde_json::Value::Null),
        );
    }
    Ok(serde_json::Value::Object(out))
}

/// Baseline-vs-compared execution comparison.
pub async fn compare_data(
    ctx: &ApiContext,
    baseline: &str,
    compared: &str,
) -> CliResult<serde_json::Value> {
    let cmp = performance::compare(ctx, baseline, compared).await?;
    Ok(serde_json::to_value(&cmp)?)
}

/// Progress metrics for one execution.
pub async fn progress_data(ctx: &ApiContext, id: &str) -> CliResult<serde_json::Value> {
    let metrics = progress::get_progress(ctx, id).await?;
    Ok(serde_json::to_value(&metrics)?)
}
