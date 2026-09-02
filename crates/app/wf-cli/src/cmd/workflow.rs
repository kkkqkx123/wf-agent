use std::path::Path;

use wf_api::workflow::{self, graph_query, search, summary, version, versioning};

use crate::args::{Cli, WorkflowSub, WorkflowVersionSub};
use crate::cmd::render::render_envelope;
use crate::error::{CliError, CliResult};
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &WorkflowSub) -> CliResult<()> {
    let adapter =
        crate::domain::DomainAdapter::bootstrap_for_cli(cli, crate::mode::CliMode::Run).await?;
    let ctx = adapter.api_context();

    let result = match sub {
        WorkflowSub::List {
            keyword,
            limit,
            tags,
            category,
            author,
        } => {
            if tags.is_some() || category.is_some() || author.is_some() {
                let opts = search::WorkflowSearchOptions {
                    keyword: keyword.clone(),
                    tags: tags.as_deref().map(|s| {
                        s.split(',')
                            .map(|v| v.trim().to_string())
                            .filter(|v| !v.is_empty())
                            .collect()
                    }),
                    category: category.clone(),
                    author: author.clone(),
                    limit: *limit,
                    offset: None,
                };
                let summaries = search::search_workflows(ctx, &opts).await?;
                let data = serde_json::to_value(&summaries)?;
                render_envelope(cli.output, OutputEnvelope::success("workflow-list", data))
            } else {
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
                render_envelope(cli.output, OutputEnvelope::success("workflow-list", data))
            }
        }
        WorkflowSub::Show { id } => {
            let def = workflow::get_workflow(ctx, id).await?;
            let data = serde_json::to_value(&def)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("workflow-show", data).with_entity(id.clone()),
            )
        }
        WorkflowSub::Graph {
            id,
            summary: show_summary,
            detect_cycles,
            topo,
            reachability,
            neighbors,
            node_type,
        } => {
            if *show_summary {
                let s = graph_query::graph_summary(ctx, id).await?;
                let data = serde_json::to_value(&s)?;
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("workflow-graph-summary", data).with_entity(id.clone()),
                )
            } else if *detect_cycles {
                let r = graph_query::graph_detect_cycles(ctx, id).await?;
                let data = serde_json::json!({
                    "hasCycle": r.has_cycle,
                    "cycleNodes": r.cycle_nodes,
                    "cycleEdges": r.cycle_edges,
                });
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("workflow-graph-cycles", data).with_entity(id.clone()),
                )
            } else if *topo {
                let r = graph_query::graph_topological_sort(ctx, id).await?;
                let data = serde_json::json!({
                    "success": r.success,
                    "sortedNodes": r.sorted_nodes,
                    "cycleNodes": r.cycle_nodes,
                });
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("workflow-graph-topo", data).with_entity(id.clone()),
                )
            } else if let Some(node) = reachability {
                let graph = graph_query::get_graph(ctx, id).await?;
                let analysis = wf_workflow::analysis::analyze_reachability(&graph);
                let reachable = wf_workflow::analysis::get_reachable_nodes(&graph, node);
                let data = serde_json::json!({
                    "workflowId": id,
                    "node": node,
                    "reachable": reachable.into_iter().collect::<Vec<_>>(),
                    "reachableFromStart": analysis.reachable_from_start.into_iter().collect::<Vec<_>>(),
                    "unreachableNodes": analysis.unreachable_nodes,
                });
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("workflow-graph-reachability", data)
                        .with_entity(id.clone()),
                )
            } else if let Some(node) = neighbors {
                let view = graph_query::graph_node_neighbors(ctx, id, node).await?;
                let data = serde_json::to_value(&view)?;
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("workflow-graph-neighbors", data)
                        .with_entity(id.clone()),
                )
            } else if let Some(ty) = node_type {
                let nodes = graph_query::graph_nodes_by_type(ctx, id, ty).await?;
                let data = serde_json::to_value(&nodes)?;
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("workflow-graph-nodes-by-type", data)
                        .with_entity(id.clone()),
                )
            } else {
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
        }
        WorkflowSub::Create { file, format } => {
            let def = load_workflow_file(Path::new(file), format)?;
            workflow::validate_workflow(&def)
                .map_err(|e| CliError::Arguments(format!("workflow validation failed: {e}")))?;
            workflow::save_workflow(ctx, &def).await?;
            let data = serde_json::json!({"id": def.id, "name": def.name});
            render_envelope(
                cli.output,
                OutputEnvelope::success("workflow-create", data).with_entity(def.id),
            )
        }
        WorkflowSub::Update { id, file, format } => {
            let mut def = load_workflow_file(Path::new(file), format)?;
            def.id = id.clone();
            workflow::validate_workflow(&def)
                .map_err(|e| CliError::Arguments(format!("workflow validation failed: {e}")))?;
            workflow::save_workflow(ctx, &def).await?;
            let data = serde_json::to_value(&def)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("workflow-update", data).with_entity(id.clone()),
            )
        }
        WorkflowSub::Delete { id, force: _force } => {
            let exists = workflow::workflow_exists(ctx, id).await?;
            if !exists {
                render_envelope(
                    cli.output,
                    OutputEnvelope::failure("workflow-delete", format!("workflow not found: {id}")),
                )
            } else {
                // Workflows have no cross-resource reference check today
                // (tool/script do via `check_*_delete_references`); `--force`
                // is reserved for future reference integrity and for CLI
                // compatibility with `tool/script delete --force`.
                let deleted = workflow::delete_workflow(ctx, id).await?;
                if deleted {
                    let data = serde_json::json!({"deleted": id});
                    render_envelope(
                        cli.output,
                        OutputEnvelope::success("workflow-delete", data).with_entity(id.clone()),
                    )
                } else {
                    render_envelope(
                        cli.output,
                        OutputEnvelope::failure(
                            "workflow-delete",
                            format!("workflow not found: {id}"),
                        ),
                    )
                }
            }
        }
        WorkflowSub::Clone { id, as_id } => {
            if let Some(new_id) = as_id {
                if workflow::workflow_exists(ctx, new_id).await? {
                    return Err(CliError::Arguments(format!(
                        "workflow already exists: {new_id}"
                    )));
                }
            }
            let new_id = workflow::clone_workflow(ctx, id, as_id.as_deref()).await?;
            let data = serde_json::json!({"source": id, "cloned": new_id});
            render_envelope(
                cli.output,
                OutputEnvelope::success("workflow-clone", data).with_entity(new_id.clone()),
            )
        }
        WorkflowSub::Validate { file, format } => {
            let def = load_workflow_file(Path::new(file), format)?;
            match workflow::validate_workflow(&def) {
                Ok(()) => {
                    let data = serde_json::json!({"valid": true, "id": def.id});
                    render_envelope(
                        cli.output,
                        OutputEnvelope::success("workflow-validate", data),
                    )
                }
                Err(e) => {
                    let data = serde_json::json!({"valid": false, "id": def.id, "errors": [e.to_string()]});
                    if cli.output == crate::output::OutputFormat::Text {
                        render_envelope(
                            cli.output,
                            OutputEnvelope::failure("workflow-validate", e.to_string()),
                        )
                    } else {
                        render_envelope(
                            cli.output,
                            OutputEnvelope::success("workflow-validate", data),
                        )
                    }
                }
            }
        }
        WorkflowSub::Export { id, format, file } => {
            let value = workflow::export_workflow(ctx, id).await?;
            if *format == "toml" {
                let def: wf_types::WorkflowDefinition = serde_json::from_value(value.clone())?;
                let toml_str = toml::to_string(&def)
                    .map_err(|e| CliError::Configuration(format!("toml serialize failed: {e}")))?;
                if let Some(path) = file {
                    std::fs::write(path, toml_str)
                        .map_err(|e| CliError::Configuration(format!("write failed: {e}")))?;
                    let data =
                        serde_json::json!({"exported": id, "output": path, "format": "toml"});
                    render_envelope(
                        cli.output,
                        OutputEnvelope::success("workflow-export", data).with_entity(id.clone()),
                    )
                } else if cli.output == crate::output::OutputFormat::Text {
                    println!("{toml_str}");
                    Ok(())
                } else {
                    let data = serde_json::json!({"workflowId": id, "format": "toml", "content": toml_str});
                    render_envelope(
                        cli.output,
                        OutputEnvelope::success("workflow-export", data).with_entity(id.clone()),
                    )
                }
            } else if let Some(path) = file {
                std::fs::write(path, serde_json::to_string_pretty(&value)?)
                    .map_err(|e| CliError::Configuration(format!("write failed: {e}")))?;
                let data = serde_json::json!({"exported": id, "output": path});
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("workflow-export", data).with_entity(id.clone()),
                )
            } else {
                let pretty = serde_json::to_string_pretty(&value)?;
                if cli.output == crate::output::OutputFormat::Text {
                    println!("{pretty}");
                    Ok(())
                } else {
                    render_envelope(
                        cli.output,
                        OutputEnvelope::success("workflow-export", value).with_entity(id.clone()),
                    )
                }
            }
        }
        WorkflowSub::Import { file, format } => {
            let def = load_workflow_file(Path::new(file), format)?;
            workflow::save_workflow(ctx, &def).await?;
            let data = serde_json::json!({"imported": def.id});
            render_envelope(
                cli.output,
                OutputEnvelope::success("workflow-import", data).with_entity(def.id.clone()),
            )
        }
        WorkflowSub::Version { sub } => match sub {
            WorkflowVersionSub::List { id } => {
                let versions = version::list_workflow_versions(ctx, id).await?;
                let data = serde_json::to_value(&versions)?;
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("workflow-version-list", data).with_entity(id.clone()),
                )
            }
            WorkflowVersionSub::Show { id, version: ver } => {
                let def = version::get_workflow_version(ctx, id, ver).await?;
                let data = serde_json::to_value(&def)?;
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("workflow-version-show", data)
                        .with_entity(format!("{id}:{ver}")),
                )
            }
            WorkflowVersionSub::Bump {
                id,
                level,
                changes,
                keep_original,
            } => {
                let strategy = match level.to_ascii_lowercase().as_str() {
                    "patch" => versioning::VersionStrategy::Patch,
                    "minor" => versioning::VersionStrategy::Minor,
                    "major" => versioning::VersionStrategy::Major,
                    _ => {
                        return Err(CliError::Arguments(format!(
                            "invalid level {level}: expected patch|minor|major"
                        )))
                    }
                };
                let changes_obj: versioning::WorkflowChanges = if let Some(json) = changes {
                    serde_json::from_str(json)?
                } else {
                    versioning::WorkflowChanges::default()
                };
                let new_version = versioning::create_versioned_update(
                    ctx,
                    id,
                    strategy,
                    &changes_obj,
                    *keep_original,
                )
                .await?;
                let data = serde_json::json!({"workflowId": id, "newVersion": new_version});
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("workflow-version-bump", data).with_entity(id.clone()),
                )
            }
            WorkflowVersionSub::Diff { id, from, to } => {
                let a = version::get_workflow_version(ctx, id, from).await?;
                let b = version::get_workflow_version(ctx, id, to).await?;
                let diff = diff_workflows(&a, &b);
                let data = serde_json::to_value(&diff)?;
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("workflow-version-diff", data).with_entity(id.clone()),
                )
            }
            WorkflowVersionSub::Changelog { id } => {
                let versions = version::list_workflow_versions(ctx, id).await?;
                let mut sorted = versions;
                sorted.sort_by(|a, b| {
                    a.version
                        .cmp(&b.version)
                        .then_with(|| a.updated_at.cmp(&b.updated_at))
                });
                let entries: Vec<serde_json::Value> = sorted
                    .iter()
                    .map(|v| {
                        serde_json::json!({
                            "version": v.version,
                            "id": v.id,
                            "name": v.name,
                            "updatedAt": v.updated_at,
                        })
                    })
                    .collect();
                let data = serde_json::json!({"workflowId": id, "changelog": entries});
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("workflow-version-changelog", data)
                        .with_entity(id.clone()),
                )
            }
        },
        WorkflowSub::Rollback { id, version } => {
            workflow::rollback_workflow(ctx, id, version).await?;
            let data = serde_json::json!({"workflowId": id, "rolledBackTo": version});
            render_envelope(
                cli.output,
                OutputEnvelope::success("workflow-rollback", data).with_entity(id.clone()),
            )
        }
        WorkflowSub::ExecutionGraph { id } => {
            let graph = graph_query::get_execution_graph(ctx, id).await?;
            let data = serde_json::to_value(&graph)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("workflow-execution-graph", data).with_entity(id.clone()),
            )
        }
    };

    adapter.shutdown().await?;
    result
}

fn resolve_format(path: &Path, format: &str) -> String {
    if format != "auto" {
        return format.to_ascii_lowercase();
    }
    match path.extension().and_then(|s| s.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("toml") => "toml".to_string(),
        _ => "json".to_string(),
    }
}

fn load_workflow_file(path: &Path, format: &str) -> CliResult<wf_types::WorkflowDefinition> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        CliError::Configuration(format!("read file {} failed: {e}", path.display()))
    })?;
    let fmt = resolve_format(path, format);
    match fmt.as_str() {
        "toml" => wf_config::parser::parse_toml(&content)
            .map_err(|e| CliError::Arguments(format!("invalid TOML in {}: {e}", path.display()))),
        _ => wf_config::parser::parse_json(&content)
            .map_err(|e| CliError::Arguments(format!("invalid JSON in {}: {e}", path.display()))),
    }
}

#[derive(Debug, serde::Serialize)]
struct WorkflowDiff {
    from_version: Option<String>,
    to_version: Option<String>,
    nodes_added: usize,
    nodes_removed: usize,
    nodes_modified: usize,
    edges_added: usize,
    edges_removed: usize,
    edges_modified: usize,
    added_node_ids: Vec<String>,
    removed_node_ids: Vec<String>,
    added_edge_ids: Vec<String>,
    removed_edge_ids: Vec<String>,
}

fn diff_workflows(
    a: &wf_types::WorkflowDefinition,
    b: &wf_types::WorkflowDefinition,
) -> WorkflowDiff {
    use std::collections::{BTreeMap, BTreeSet};
    let a_nodes: BTreeMap<&str, &wf_types::node::BaseStaticNode> =
        a.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let b_nodes: BTreeMap<&str, &wf_types::node::BaseStaticNode> =
        b.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let a_ids: BTreeSet<&str> = a_nodes.keys().copied().collect();
    let b_ids: BTreeSet<&str> = b_nodes.keys().copied().collect();
    let added_node_ids: Vec<String> = b_ids.difference(&a_ids).map(|s| (*s).to_string()).collect();
    let removed_node_ids: Vec<String> =
        a_ids.difference(&b_ids).map(|s| (*s).to_string()).collect();
    let mut modified = 0;
    for id in a_ids.intersection(&b_ids) {
        let av = serde_json::to_value(a_nodes[*id]).unwrap_or_default();
        let bv = serde_json::to_value(b_nodes[*id]).unwrap_or_default();
        if av != bv {
            modified += 1;
        }
    }
    let a_edges: BTreeMap<&str, &wf_types::workflow::Edge> =
        a.edges.iter().map(|e| (e.id.as_str(), e)).collect();
    let b_edges: BTreeMap<&str, &wf_types::workflow::Edge> =
        b.edges.iter().map(|e| (e.id.as_str(), e)).collect();
    let a_eids: BTreeSet<&str> = a_edges.keys().copied().collect();
    let b_eids: BTreeSet<&str> = b_edges.keys().copied().collect();
    let added_edge_ids: Vec<String> = b_eids
        .difference(&a_eids)
        .map(|s| (*s).to_string())
        .collect();
    let removed_edge_ids: Vec<String> = a_eids
        .difference(&b_eids)
        .map(|s| (*s).to_string())
        .collect();
    let mut edges_modified = 0;
    for id in a_eids.intersection(&b_eids) {
        let av = serde_json::to_value(a_edges[*id]).unwrap_or_default();
        let bv = serde_json::to_value(b_edges[*id]).unwrap_or_default();
        if av != bv {
            edges_modified += 1;
        }
    }
    WorkflowDiff {
        from_version: a.version.clone(),
        to_version: b.version.clone(),
        nodes_added: added_node_ids.len(),
        nodes_removed: removed_node_ids.len(),
        nodes_modified: modified,
        edges_added: added_edge_ids.len(),
        edges_removed: removed_edge_ids.len(),
        edges_modified,
        added_node_ids,
        removed_node_ids,
        added_edge_ids,
        removed_edge_ids,
    }
}
