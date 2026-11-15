//! Text views for execution hierarchy, subtree and history.
//!
//! The server owns the typed views; the CLI prints the JSON shapes it
//! returns, keeping the established text layout.

use crate::args::Cli;
use crate::cmd::render::print_line_to_cli;
use crate::error::CliResult;

/// Wire name of the engine that owns an execution.
pub(crate) fn execution_type_label(kind: &str) -> &'static str {
    match kind {
        "workflow" => "workflow",
        "agent" => "agent",
        "chat" => "chat",
        _ => "unknown",
    }
}

fn str_field<'a>(value: &'a serde_json::Value, name: &str) -> &'a str {
    value.get(name).and_then(|v| v.as_str()).unwrap_or("-")
}

/// One execution's place in the parent/child tree.
pub(crate) fn print_hierarchy(cli: &Cli, view: &serde_json::Value) -> CliResult<()> {
    let execution_id = str_field(view, "execution_id");
    let execution_type = view
        .get("execution_type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let status = str_field(view, "status");
    print_line_to_cli(
        cli,
        &format!(
            "{} ({}) {}",
            execution_id,
            execution_type_label(execution_type),
            status
        ),
    )?;
    print_line_to_cli(
        cli,
        &format!(
            "  depth: {}",
            view.get("depth").and_then(|v| v.as_u64()).unwrap_or(0)
        ),
    )?;
    match view.get("parent") {
        Some(parent) if !parent.is_null() => print_line_to_cli(
            cli,
            &format!(
                "  parent: {} ({})",
                str_field(parent, "execution_id"),
                execution_type_label(
                    parent
                        .get("execution_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                )
            ),
        )?,
        _ => print_line_to_cli(cli, "  parent: -")?,
    }
    if let Some(root) = view.get("root") {
        print_line_to_cli(
            cli,
            &format!(
                "  root: {} ({})",
                str_field(root, "execution_id"),
                execution_type_label(
                    root.get("execution_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                )
            ),
        )?;
    }
    match view.get("ancestors").and_then(|v| v.as_array()) {
        Some(ancestors) if !ancestors.is_empty() => {
            let chain = ancestors
                .iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join(" -> ");
            print_line_to_cli(cli, &format!("  ancestors: {chain}"))?;
        }
        _ => print_line_to_cli(cli, "  ancestors: -")?,
    }
    print_line_to_cli(
        cli,
        &format!("  children: see `wf execution subtree {execution_id}`"),
    )?;
    Ok(())
}

/// A subtree, one execution per line, indented by depth.
pub(crate) fn print_subtree(cli: &Cli, tree: &serde_json::Value) -> CliResult<()> {
    let nodes = tree
        .get("nodes")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    for node in &nodes {
        let depth = node.get("depth").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let marker = if depth == 0 { "" } else { "  " };
        let status = node.get("status").and_then(|v| v.as_str()).unwrap_or("-");
        let execution_type = node
            .get("execution_type")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        print_line_to_cli(
            cli,
            &format!(
                "{}{} {} [{}] {}",
                marker.repeat(depth),
                str_field(node, "execution_id"),
                execution_type_label(execution_type),
                status,
                tree_label(depth as u32)
            ),
        )?;
    }
    if tree
        .get("truncated")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        let omitted = tree.get("omitted").and_then(|v| v.as_u64()).unwrap_or(0);
        match tree.get("next_offset").and_then(|v| v.as_u64()) {
            Some(next) => print_line_to_cli(
                cli,
                &format!(
                    "{} nodes shown, {} more remain; continue with `--limit {} --cursor {next}`",
                    nodes.len(),
                    omitted,
                    nodes.len(),
                ),
            )?,
            None => print_line_to_cli(
                cli,
                &format!(
                    "{} nodes shown, {} more remain; narrow the query or inspect a child directly",
                    nodes.len(),
                    omitted,
                ),
            )?,
        }
    }
    Ok(())
}

/// The parent/child relationship label of one subtree row.
pub(crate) fn tree_label(depth: u32) -> &'static str {
    match depth {
        0 => "root",
        1 => "child",
        _ => "descendant",
    }
}

/// An execution's recorded history, one section per heading.
pub(crate) fn print_history(cli: &Cli, view: &serde_json::Value) -> CliResult<()> {
    let execution_id = str_field(view, "execution_id");
    let execution_type = view
        .get("execution_type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    print_line_to_cli(
        cli,
        &format!(
            "{} ({})",
            execution_id,
            execution_type_label(execution_type)
        ),
    )?;

    if let Some(timeline) = view.get("timeline").and_then(|v| v.as_array()) {
        if !timeline.is_empty() {
            print_line_to_cli(cli, &format!("timeline ({}):", timeline.len()))?;
            for event in timeline {
                print_line_to_cli(
                    cli,
                    &format!(
                        "  [{}] {}",
                        event.get("timestamp").and_then(|v| v.as_i64()).unwrap_or(0),
                        event.get("type").and_then(|v| v.as_str()).unwrap_or("?")
                    ),
                )?;
            }
        }
    }

    if let Some(nodes) = view.get("node_executions").and_then(|v| v.as_array()) {
        if !nodes.is_empty() {
            print_line_to_cli(cli, &format!("nodes ({}):", nodes.len()))?;
            for node in nodes {
                print_line_to_cli(
                    cli,
                    &format!(
                        "  {} {} {}ms",
                        str_field(node, "node_id"),
                        str_field(node, "node_type"),
                        node.get("duration_ms")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0)
                    ),
                )?;
            }
        }
    }

    if let Some(iterations) = view.get("iterations").and_then(|v| v.as_array()) {
        if !iterations.is_empty() {
            print_line_to_cli(cli, &format!("iterations ({}):", iterations.len()))?;
            for iteration in iterations {
                print_line_to_cli(
                    cli,
                    &format!(
                        "  #{} {}ms {} tools",
                        iteration
                            .get("iteration")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0),
                        iteration
                            .get("duration")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0),
                        iteration
                            .get("tool_call_count")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0)
                    ),
                )?;
            }
        }
    }

    if let Some(variables) = view.get("variables").and_then(|v| v.as_object()) {
        if !variables.is_empty() {
            print_line_to_cli(cli, &format!("variables ({}):", variables.len()))?;
            for (name, value) in variables {
                print_line_to_cli(cli, &format!("  {name} = {value}"))?;
            }
        }
    }

    if let Some(context) = view.get("context_evolution").and_then(|v| v.as_array()) {
        if !context.is_empty() {
            print_line_to_cli(cli, &format!("context ({}):", context.len()))?;
            for entry in context {
                print_line_to_cli(
                    cli,
                    &format!(
                        "  [{}] #{} {}",
                        entry.get("timestamp").and_then(|v| v.as_i64()).unwrap_or(0),
                        entry.get("iteration").and_then(|v| v.as_u64()).unwrap_or(0),
                        entry
                            .get("description")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                    ),
                )?;
            }
        }
    }

    if let Some(transitions) = view.get("status_transitions").and_then(|v| v.as_array()) {
        if !transitions.is_empty() {
            print_line_to_cli(cli, &format!("transitions ({}):", transitions.len()))?;
            for transition in transitions {
                print_line_to_cli(
                    cli,
                    &format!(
                        "  [{}] {} -> {}",
                        transition
                            .get("timestamp")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0),
                        transition
                            .get("from")
                            .and_then(|v| v.as_str())
                            .unwrap_or("?"),
                        transition.get("to").and_then(|v| v.as_str()).unwrap_or("?")
                    ),
                )?;
            }
        }
    }
    Ok(())
}
