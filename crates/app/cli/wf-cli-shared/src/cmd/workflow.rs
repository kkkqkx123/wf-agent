use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::args::{Cli, WorkflowSub, WorkflowVersionSub};
use crate::cmd::render::render_envelope;
use crate::connection::ClientHandle;
use crate::error::{CliError, CliResult};
use crate::output::OutputEnvelope;
use wf_client::WfClient;

pub async fn run(cli: &Cli, sub: &WorkflowSub) -> CliResult<()> {
    let handle = ClientHandle::from_cli(cli).await?;
    run_with_client(cli, sub, handle.client()).await
}

pub(crate) async fn run_with_client(
    cli: &Cli,
    sub: &WorkflowSub,
    client: &WfClient,
) -> CliResult<()> {
    match sub {
        WorkflowSub::List {
            keyword,
            limit,
            tags,
            category,
            author,
        } => {
            // Filtering and paging live server-side: the search endpoint
            // already supports keyword, tags, category, author and limit.
            let mut params: Vec<String> = Vec::new();
            if let Some(q) = keyword {
                params.push(format!("q={q}"));
            }
            if let Some(t) = tags {
                params.push(format!("tags={t}"));
            }
            if let Some(cat) = category {
                params.push(format!("category={cat}"));
            }
            if let Some(a) = author {
                params.push(format!("author={a}"));
            }
            if let Some(lim) = limit {
                params.push(format!("limit={lim}"));
            }
            let query = if params.is_empty() {
                String::new()
            } else {
                format!("?{}", params.join("&"))
            };
            let summaries: serde_json::Value = client
                .get_json(&format!("/api/v1/workflows/search{query}"))
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("workflow-list", summaries),
            )
        }
        WorkflowSub::Show { id } => {
            let data: serde_json::Value =
                client.get_json(&format!("/api/v1/workflows/{id}")).await?;
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
                let envelope = client
                    .generated()
                    .handle_graph_summary(id)
                    .await
                    .map_err(wf_client::client::into_typed_client_error)?;
                let view: wf_client::models::GraphSummaryDoc =
                    wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
                let data = serde_json::to_value(&view).unwrap_or(serde_json::Value::Null);
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("workflow-graph-summary", data).with_entity(id.clone()),
                )
            } else if *detect_cycles {
                let envelope = client
                    .generated()
                    .handle_graph_cycles(id)
                    .await
                    .map_err(wf_client::client::into_typed_client_error)?;
                let view: wf_client::models::CycleDetectionDoc =
                    wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
                let data = serde_json::json!({
                    "hasCycle": view.has_cycle,
                    "cycleNodes": view.cycle_nodes,
                    "cycleEdges": view.cycle_edges,
                });
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("workflow-graph-cycles", data).with_entity(id.clone()),
                )
            } else if *topo {
                let envelope = client
                    .generated()
                    .handle_graph_topology(id)
                    .await
                    .map_err(wf_client::client::into_typed_client_error)?;
                let view: wf_client::models::TopologicalSortDoc =
                    wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
                let data = serde_json::json!({
                    "success": view.success,
                    "sortedNodes": view.sorted_nodes,
                    "cycleNodes": view.cycle_nodes,
                });
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("workflow-graph-topo", data).with_entity(id.clone()),
                )
            } else if let Some(node) = reachability {
                let graph_envelope = client
                    .generated()
                    .handle_graph(id)
                    .await
                    .map_err(wf_client::client::into_typed_client_error)?;
                let graph: serde_json::Value =
                    wf_client::client::unwrap_generated_envelope(graph_envelope.into_inner())?;
                let reach_envelope = client
                    .generated()
                    .handle_graph_reachability(id)
                    .await
                    .map_err(wf_client::client::into_typed_client_error)?;
                let analysis: wf_client::models::ReachabilityDoc =
                    wf_client::client::unwrap_generated_envelope(reach_envelope.into_inner())?;
                let reachable = reachable_nodes(&graph, node);
                let data = serde_json::json!({
                    "workflowId": id,
                    "node": node,
                    "reachable": reachable,
                    "reachableFromStart": analysis.reachable_from_start,
                    "unreachableNodes": analysis.unreachable_nodes,
                });
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("workflow-graph-reachability", data)
                        .with_entity(id.clone()),
                )
            } else if let Some(node) = neighbors {
                let envelope = client
                    .generated()
                    .handle_graph_neighbors(id, node)
                    .await
                    .map_err(wf_client::client::into_typed_client_error)?;
                let view: wf_client::models::GraphNeighborsDoc =
                    wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
                let data = serde_json::to_value(&view).unwrap_or(serde_json::Value::Null);
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("workflow-graph-neighbors", data)
                        .with_entity(id.clone()),
                )
            } else if let Some(ty) = node_type {
                let envelope = client
                    .generated()
                    .handle_graph_nodes(id, Some(ty.as_str()))
                    .await
                    .map_err(wf_client::client::into_typed_client_error)?;
                let nodes: Vec<wf_client::models::GraphNodeDoc> =
                    wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
                let data = serde_json::to_value(&nodes).unwrap_or(serde_json::Value::Null);
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("workflow-graph-nodes-by-type", data)
                        .with_entity(id.clone()),
                )
            } else {
                let nodes_envelope = client
                    .generated()
                    .handle_graph_nodes(id, None)
                    .await
                    .map_err(wf_client::client::into_typed_client_error)?;
                let nodes: Vec<wf_client::models::GraphNodeDoc> =
                    wf_client::client::unwrap_generated_envelope(nodes_envelope.into_inner())?;
                let edges_envelope = client
                    .generated()
                    .handle_graph_edges(id)
                    .await
                    .map_err(wf_client::client::into_typed_client_error)?;
                let edges: Vec<wf_client::models::GraphEdgeDoc> =
                    wf_client::client::unwrap_generated_envelope(edges_envelope.into_inner())?;
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
            let body = serde_json::to_value(&def)?;
            let envelope = client
                .generated()
                .handle_create_workflow(&body)
                .await
                .map_err(wf_client::client::into_typed_client_error)?;
            let data: serde_json::Value =
                wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
            let wf_id = data
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or(def.id.as_str())
                .to_string();
            let data = serde_json::json!({"id": wf_id, "name": def.name});
            render_envelope(
                cli.output,
                OutputEnvelope::success("workflow-create", data).with_entity(wf_id),
            )
        }
        WorkflowSub::Update { id, file, format } => {
            let mut def = load_workflow_file(Path::new(file), format)?;
            def.id = id.clone();
            let body = serde_json::to_value(&def)?;
            let data: serde_json::Value = client
                .put_json(&format!("/api/v1/workflows/{id}"), &body)
                .await?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("workflow-update", data).with_entity(id.clone()),
            )
        }
        WorkflowSub::Delete { id, force: _force } => {
            match client
                .get_json::<serde_json::Value>(&format!("/api/v1/workflows/{id}"))
                .await
            {
                Err(_) => render_envelope(
                    cli.output,
                    OutputEnvelope::failure("workflow-delete", format!("workflow not found: {id}")),
                ),
                Ok(_) => {
                    // Workflows have no cross-resource reference check today
                    // (tool/script do server-side); `--force` is reserved for
                    // future reference integrity and CLI compatibility.
                    let deleted: bool = client
                        .delete_json(&format!("/api/v1/workflows/{id}"))
                        .await?;
                    if deleted {
                        let data = serde_json::json!({"deleted": id});
                        render_envelope(
                            cli.output,
                            OutputEnvelope::success("workflow-delete", data)
                                .with_entity(id.clone()),
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
        }
        WorkflowSub::Clone { id, as_id } => {
            if let Some(new_id) = as_id {
                let check: Result<serde_json::Value, _> = client
                    .generated()
                    .handle_get_workflow(new_id)
                    .await
                    .map_err(wf_client::client::into_typed_client_error)
                    .and_then(|e| wf_client::client::unwrap_generated_envelope(e.into_inner()));
                if check.is_ok() {
                    return Err(CliError::Arguments(format!(
                        "workflow already exists: {new_id}"
                    )));
                }
            }
            // Named request body; response stays passthrough JSON.
            let body = wf_client::models::CloneBody {
                new_id: as_id.clone(),
            };
            let envelope = client
                .generated()
                .handle_clone_workflow(id, &body)
                .await
                .map_err(wf_client::client::into_typed_client_error)?;
            let new_id: String =
                wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
            let data = serde_json::json!({"source": id, "cloned": new_id});
            render_envelope(
                cli.output,
                OutputEnvelope::success("workflow-clone", data).with_entity(new_id),
            )
        }
        WorkflowSub::Validate { file, format } => {
            let def = load_workflow_file(Path::new(file), format)?;
            let body = serde_json::to_value(&def)?;
            match client
                .generated()
                .handle_validate_workflow(&body)
                .await
                .map_err(wf_client::client::into_typed_client_error)
                .and_then(|e| {
                    wf_client::client::unwrap_generated_envelope::<_, serde_json::Value>(
                        e.into_inner(),
                    )
                }) {
                Ok(_) => {
                    let data = serde_json::json!({"valid": true, "id": def.id});
                    render_envelope(
                        cli.output,
                        OutputEnvelope::success("workflow-validate", data),
                    )
                }
                Err(e) => {
                    let msg = e.to_string();
                    if cli.output == crate::output::OutputFormat::Text {
                        render_envelope(
                            cli.output,
                            OutputEnvelope::failure("workflow-validate", msg),
                        )
                    } else {
                        let data =
                            serde_json::json!({"valid": false, "id": def.id, "errors": [msg]});
                        render_envelope(
                            cli.output,
                            OutputEnvelope::success("workflow-validate", data),
                        )
                    }
                }
            }
        }
        WorkflowSub::Export { id, format, file } => {
            let value: serde_json::Value = client
                .get_json(&format!("/api/v1/workflows/{id}/export"))
                .await?;
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
            // Named request body; response stays passthrough JSON.
            let json = serde_json::to_string(&def).unwrap_or_default();
            let body = wf_client::models::ImportBody { json };
            let envelope = client
                .generated()
                .handle_import_workflow(&body)
                .await
                .map_err(wf_client::client::into_typed_client_error)?;
            let data: serde_json::Value =
                wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
            let wf_id = data
                .get("imported")
                .and_then(|v| v.as_str())
                .unwrap_or(def.id.as_str())
                .to_string();
            let data = serde_json::json!({"imported": wf_id});
            render_envelope(
                cli.output,
                OutputEnvelope::success("workflow-import", data).with_entity(wf_id),
            )
        }
        WorkflowSub::Version { sub } => match sub {
            WorkflowVersionSub::List { id } => {
                let data: serde_json::Value = client
                    .get_json(&format!("/api/v1/workflows/{id}/versions"))
                    .await?;
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("workflow-version-list", data).with_entity(id.clone()),
                )
            }
            WorkflowVersionSub::Show { id, version: ver } => {
                let data: serde_json::Value = client
                    .get_json(&format!("/api/v1/workflows/{id}/versions/{ver}"))
                    .await?;
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
                match level.to_ascii_lowercase().as_str() {
                    "patch" | "minor" | "major" => {}
                    _ => {
                        return Err(CliError::Arguments(format!(
                            "invalid level {level}: expected patch|minor|major"
                        )))
                    }
                }
                let changes_obj: serde_json::Value = if let Some(json) = changes {
                    serde_json::from_str(json)?
                } else {
                    serde_json::json!({})
                };
                let body = serde_json::json!({
                    "changes": changes_obj,
                    "keep_original": keep_original,
                });
                let new_version: String = client
                    .post_json(
                        &format!("/api/v1/workflows/{id}/versions/increment?level={level}"),
                        &body,
                    )
                    .await?;
                let data = serde_json::json!({"workflowId": id, "newVersion": new_version});
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("workflow-version-bump", data).with_entity(id.clone()),
                )
            }
            WorkflowVersionSub::Diff { id, from, to } => {
                let a: serde_json::Value = client
                    .get_json(&format!("/api/v1/workflows/{id}/versions/{from}"))
                    .await?;
                let b: serde_json::Value = client
                    .get_json(&format!("/api/v1/workflows/{id}/versions/{to}"))
                    .await?;
                let diff = diff_workflow_values(&a, &b);
                let data = serde_json::to_value(&diff)?;
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("workflow-version-diff", data).with_entity(id.clone()),
                )
            }
            WorkflowVersionSub::Changelog { id } => {
                let data: serde_json::Value = client
                    .get_json(&format!("/api/v1/workflows/{id}/versions"))
                    .await?;
                let mut versions = data.as_array().cloned().unwrap_or_default();
                versions.sort_by(|a, b| {
                    a.get("version")
                        .and_then(|v| v.as_str())
                        .cmp(&b.get("version").and_then(|v| v.as_str()))
                        .then_with(|| {
                            a.get("updated_at")
                                .and_then(|v| v.as_i64())
                                .cmp(&b.get("updated_at").and_then(|v| v.as_i64()))
                        })
                });
                let entries: Vec<serde_json::Value> = versions
                    .iter()
                    .map(|v| {
                        serde_json::json!({
                            "version": v.get("version"),
                            "id": v.get("id"),
                            "name": v.get("name"),
                            "updatedAt": v.get("updated_at"),
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
            let body = serde_json::json!({ "version": version });
            client
                .post_json::<_, serde_json::Value>(
                    &format!("/api/v1/workflows/{id}/rollback"),
                    &body,
                )
                .await?;
            let data = serde_json::json!({"workflowId": id, "rolledBackTo": version});
            render_envelope(
                cli.output,
                OutputEnvelope::success("workflow-rollback", data).with_entity(id.clone()),
            )
        }
        WorkflowSub::ExecutionGraph {
            id,
            analysis,
            slow_nodes,
            percentile,
            efficiency,
            path_probability,
            alternative_paths,
        } => {
            let wants_analysis =
                *analysis || *slow_nodes || *efficiency || *path_probability || *alternative_paths;
            if wants_analysis {
                // Escape hatch: execution graph analyses are dynamic observation queries.
                let mut out = serde_json::Map::new();
                if *analysis {
                    let a: serde_json::Value = client
                        .get_json(&crate::paths::executions_id_analysis_paths(id))
                        .await?;
                    out.insert("analysis".into(), a);
                }
                if *slow_nodes {
                    // CLI flag is a 0-1 fraction; the server takes 0-100.
                    let pct = percentile * 100.0;
                    let slow: serde_json::Value = client
                        .get_json(&crate::paths::executions_id_analysis_slow_nodes(id, pct))
                        .await?;
                    out.insert("slowNodes".into(), slow);
                }
                if *efficiency {
                    let eff: serde_json::Value = client
                        .get_json(&crate::paths::executions_id_analysis_efficiency(id))
                        .await?;
                    out.insert("efficiency".into(), eff);
                }
                if *path_probability {
                    let prob: serde_json::Value = client
                        .get_json(&crate::paths::executions_id_analysis_probabilities(id))
                        .await?;
                    out.insert("pathProbability".into(), prob);
                }
                if *alternative_paths {
                    let alt: serde_json::Value = client
                        .get_json(&crate::paths::executions_id_analysis_alternatives(id))
                        .await?;
                    out.insert("alternativePaths".into(), alt);
                }
                // Always include the raw graph alongside analyses for context.
                if let Ok(graph) = client
                    .get_json::<serde_json::Value>(&crate::paths::executions_id_graph(id))
                    .await
                {
                    out.insert("graph".into(), graph);
                }
                let data = serde_json::Value::Object(out);
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("workflow-execution-graph-analysis", data)
                        .with_entity(id.clone()),
                )
            } else {
                // Escape hatch: raw execution graph is a dynamic topology payload.
                let data: serde_json::Value = client
                    .get_json(&crate::paths::executions_id_graph(id))
                    .await?;
                render_envelope(
                    cli.output,
                    OutputEnvelope::success("workflow-execution-graph", data)
                        .with_entity(id.clone()),
                )
            }
        }
    }
}

/// Nodes reachable from `start` following directed edges in a workflow
/// topology document (`{nodes, edges}` with `source_node_id` targets).
/// Presentation-only traversal over the server-fetched graph for the
/// per-node view; the server owns global reachability analysis and all
/// execution semantics.
fn reachable_nodes(graph: &serde_json::Value, start: &str) -> Vec<String> {
    let edges = graph
        .get("edges")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut adjacency: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for edge in &edges {
        let source = edge
            .get("source_node_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let target = edge
            .get("target_node_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if !source.is_empty() && !target.is_empty() {
            adjacency
                .entry(source.to_string())
                .or_default()
                .push(target.to_string());
        }
    }
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut stack = vec![start.to_string()];
    while let Some(node) = stack.pop() {
        if !seen.insert(node.clone()) {
            continue;
        }
        if let Some(next) = adjacency.get(&node) {
            stack.extend(next.iter().cloned());
        }
    }
    seen.remove(start);
    seen.into_iter().collect()
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
        "toml" => toml::from_str(&content)
            .map_err(|e| CliError::Arguments(format!("invalid TOML in {}: {e}", path.display()))),
        _ => serde_json::from_str(&content)
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

/// Presentation-only diff of two server-fetched version documents for the
/// read-only version viewer; versioning itself lives server-side.
fn diff_workflow_values(a: &serde_json::Value, b: &serde_json::Value) -> WorkflowDiff {
    let a_nodes = a
        .get("nodes")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let b_nodes = b
        .get("nodes")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let id_of = |v: &serde_json::Value| {
        v.get("id")
            .and_then(|id| id.as_str())
            .unwrap_or("")
            .to_string()
    };
    let a_ids: BTreeSet<String> = a_nodes.iter().map(id_of).collect();
    let b_ids: BTreeSet<String> = b_nodes.iter().map(id_of).collect();
    let added_node_ids: Vec<String> = b_ids.difference(&a_ids).cloned().collect();
    let removed_node_ids: Vec<String> = a_ids.difference(&b_ids).cloned().collect();
    let a_map: BTreeMap<String, &serde_json::Value> =
        a_nodes.iter().map(|n| (id_of(n), n)).collect();
    let b_map: BTreeMap<String, &serde_json::Value> =
        b_nodes.iter().map(|n| (id_of(n), n)).collect();
    let mut modified = 0;
    for id in a_ids.intersection(&b_ids) {
        if a_map[id] != b_map[id] {
            modified += 1;
        }
    }
    let a_edges = a
        .get("edges")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let b_edges = b
        .get("edges")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let a_eids: BTreeSet<String> = a_edges.iter().map(id_of).collect();
    let b_eids: BTreeSet<String> = b_edges.iter().map(id_of).collect();
    let added_edge_ids: Vec<String> = b_eids.difference(&a_eids).cloned().collect();
    let removed_edge_ids: Vec<String> = a_eids.difference(&b_eids).cloned().collect();
    let a_emap: BTreeMap<String, &serde_json::Value> =
        a_edges.iter().map(|e| (id_of(e), e)).collect();
    let b_emap: BTreeMap<String, &serde_json::Value> =
        b_edges.iter().map(|e| (id_of(e), e)).collect();
    let mut edges_modified = 0;
    for id in a_eids.intersection(&b_eids) {
        if a_emap[id] != b_emap[id] {
            edges_modified += 1;
        }
    }
    WorkflowDiff {
        from_version: a.get("version").and_then(|v| v.as_str()).map(String::from),
        to_version: b.get("version").and_then(|v| v.as_str()).map(String::from),
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
