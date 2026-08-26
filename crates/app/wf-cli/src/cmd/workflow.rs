use std::io::Write;

use wf_api::workflow::{self, graph_query, summary};

use crate::args::{Cli, WorkflowSub};
use crate::error::CliResult;
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &WorkflowSub) -> CliResult<()> {
    let adapter = crate::domain::DomainAdapter::bootstrap_for_cli(cli, crate::mode::CliMode::Run)
        .await?;
    let ctx = adapter.api_context();

    let result = match sub {
        WorkflowSub::List { keyword, limit } => {
            let summaries = summary::workflow_summaries(ctx, None).await?;
            let filtered: Vec<&summary::WorkflowSummary> = if let Some(kw) = keyword {
                let kw_lower = kw.to_lowercase();
                summaries
                    .iter()
                    .filter(|s| {
                        s.name.to_lowercase().contains(&kw_lower)
                            || s.description
                                .as_ref()
                                .map(|d| d.to_lowercase().contains(&kw_lower))
                                .unwrap_or(false)
                    })
                    .collect()
            } else {
                summaries.iter().collect()
            };
            let limited: Vec<&summary::WorkflowSummary> = if let Some(lim) = *limit {
                filtered.into_iter().take(lim as usize).collect()
            } else {
                filtered
            };
            let data = serde_json::to_value(&limited)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("workflow-list", data),
            )
        }
        WorkflowSub::Show { id } => {
            let def = workflow::get_workflow(ctx, id).await?;
            let data = serde_json::to_value(&def)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("workflow-show", data).with_entity(id.clone()),
            )
        }
        WorkflowSub::Graph { id } => {
            let nodes = graph_query::graph_nodes(ctx, id).await?;
            let edges = graph_query::graph_edges(ctx, id).await?;
            let data = serde_json::json!({
                "workflowId": id,
                "nodes": nodes,
                "edges": edges,
            });
            render_envelope(
                cli.output,
                OutputEnvelope::success("workflow-graph", data).with_entity(id.clone()),
            )
        }
    };

    adapter.shutdown().await?;
    result
}

fn render_envelope(format: crate::output::OutputFormat, envelope: OutputEnvelope) -> CliResult<()> {
    match format {
        crate::output::OutputFormat::Text => {
            let text = envelope.render(format);
            if let Some(line) = text {
                let mut stdout = std::io::stdout();
                writeln!(stdout, "{line}")?;
            }
            Ok(())
        }
        crate::output::OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&envelope)?;
            let mut stdout = std::io::stdout();
            writeln!(stdout, "{json}")?;
            Ok(())
        }
        crate::output::OutputFormat::JsonLines => {
            let json = serde_json::to_string(&envelope)?;
            let mut stdout = std::io::stdout();
            writeln!(stdout, "{json}")?;
            Ok(())
        }
        crate::output::OutputFormat::Silent => Ok(()),
    }
}