use wf_api::analysis::{error_analysis, performance, progress};

use crate::args::{AnalysisSub, Cli};
use crate::cmd::render::render_envelope;
use crate::error::CliResult;
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &AnalysisSub) -> CliResult<()> {
    let adapter =
        crate::domain::DomainAdapter::bootstrap_for_cli(cli, crate::mode::CliMode::Run).await?;
    let ctx = adapter.api_context();
    let result = match sub {
        AnalysisSub::Performance { id } => {
            let profile = performance::analyze_performance(ctx, id).await?;
            let data = serde_json::to_value(&profile)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("analysis-performance", data).with_entity(id.clone()),
            )
        }
        AnalysisSub::Bottleneck { id } => {
            let bottlenecks = performance::identify_bottlenecks(ctx, id).await?;
            let data = serde_json::to_value(&bottlenecks)?;
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
            let mut out = serde_json::Map::new();
            let stats = error_analysis::workflow_error_stats(ctx, id).await?;
            out.insert(
                "stats".into(),
                serde_json::to_value(&stats).unwrap_or(serde_json::Value::Null),
            );
            if *chain {
                let chain_data = error_analysis::get_error_chain(ctx, id, None).await?;
                out.insert(
                    "chain".into(),
                    serde_json::to_value(&chain_data).unwrap_or(serde_json::Value::Null),
                );
            }
            if *root_cause {
                let rc = error_analysis::analyze_root_cause(ctx, id).await?;
                out.insert(
                    "rootCause".into(),
                    serde_json::to_value(&rc).unwrap_or(serde_json::Value::Null),
                );
            }
            if *recovery {
                let recs = error_analysis::recovery_recommendations(ctx, id).await?;
                out.insert(
                    "recovery".into(),
                    serde_json::to_value(&recs).unwrap_or(serde_json::Value::Null),
                );
            }
            if !*chain && !*root_cause && !*recovery {
                let advanced = error_analysis::get_advanced_error_analysis(ctx, id).await?;
                out.insert(
                    "advanced".into(),
                    serde_json::to_value(&advanced).unwrap_or(serde_json::Value::Null),
                );
            }
            let data = serde_json::Value::Object(out);
            render_envelope(
                cli.output,
                OutputEnvelope::success("analysis-errors", data).with_entity(id.clone()),
            )
        }
        AnalysisSub::Compare { baseline, compared } => {
            let cmp = performance::compare(ctx, baseline, compared).await?;
            let data = serde_json::to_value(&cmp)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("analysis-compare", data)
                    .with_entity(format!("{baseline}:{compared}")),
            )
        }
        AnalysisSub::Progress { id } => {
            let metrics = progress::get_progress(ctx, id).await?;
            let data = serde_json::to_value(&metrics)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("analysis-progress", data).with_entity(id.clone()),
            )
        }
    };
    adapter.shutdown().await?;
    result
}
