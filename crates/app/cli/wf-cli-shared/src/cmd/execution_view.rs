//! Text views for execution hierarchy, subtree and history.

use wf_api::execution_hierarchy::{ExecutionHierarchyView, ExecutionSubtree};
use wf_api::execution_history::ExecutionHistoryView;
use wf_types::execution::ExecutionType;

/// Wire name of the engine that owns an execution.
pub(crate) fn execution_type_label(kind: &ExecutionType) -> &'static str {
    match kind {
        ExecutionType::Workflow => "workflow",
        ExecutionType::AgentLoop => "agent_loop",
    }
}

/// One execution's place in the parent/child tree.
pub(crate) fn print_hierarchy(view: &ExecutionHierarchyView) {
    println!(
        "{} ({}) {}",
        view.execution_id,
        execution_type_label(&view.execution_type),
        view.status.as_str()
    );
    println!("  depth: {}", view.depth);
    match &view.parent {
        Some(parent) => println!(
            "  parent: {} ({})",
            parent.execution_id,
            execution_type_label(&parent.execution_type)
        ),
        None => println!("  parent: -"),
    }
    println!(
        "  root: {} ({})",
        view.root.execution_id,
        execution_type_label(&view.root.execution_type)
    );
    println!(
        "  ancestors: {}",
        if view.ancestors.is_empty() {
            "-".to_string()
        } else {
            view.ancestors.join(" -> ")
        }
    );
    println!(
        "  children: see `wf execution subtree {}`",
        view.execution_id
    );
}

/// A subtree, one execution per line, indented by depth.
pub(crate) fn print_subtree(tree: &ExecutionSubtree) {
    for node in &tree.nodes {
        let marker = if node.depth == 0 { "" } else { "  " };
        let status = node
            .status
            .as_ref()
            .map(|status| status.as_str().to_string())
            .unwrap_or_else(|| "-".to_string());
        println!(
            "{}{} {} [{}] {}",
            marker.repeat(node.depth as usize),
            node.execution_id,
            execution_type_label(&node.execution_type),
            status,
            tree_label(node.depth)
        );
    }
    if tree.truncated {
        match tree.next_offset {
            Some(next) => println!(
                "{} nodes shown, {} more remain; continue with `--limit {} --cursor {next}`",
                tree.nodes.len(),
                tree.omitted,
                tree.nodes.len(),
            ),
            None => println!(
                "{} nodes shown, {} more remain; narrow the query or inspect a child directly",
                tree.nodes.len(),
                tree.omitted,
            ),
        }
    }
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
pub(crate) fn print_history(view: &ExecutionHistoryView) {
    println!(
        "{} ({})",
        view.execution_id,
        execution_type_label(&view.execution_type)
    );

    if !view.timeline.is_empty() {
        println!("timeline ({}):", view.timeline.len());
        for event in &view.timeline {
            println!("  [{}] {}", event.timestamp, event.r#type.as_str());
        }
    }

    if !view.node_executions.is_empty() {
        println!("nodes ({}):", view.node_executions.len());
        for node in &view.node_executions {
            println!(
                "  {} {} {}ms",
                node.node_id, node.node_type, node.duration_ms
            );
        }
    }

    if !view.iterations.is_empty() {
        println!("iterations ({}):", view.iterations.len());
        for iteration in &view.iterations {
            println!(
                "  #{} {}ms {} tools",
                iteration.iteration, iteration.duration, iteration.tool_call_count
            );
        }
    }

    if !view.variables.is_empty() {
        println!("variables ({}):", view.variables.len());
        for (name, value) in &view.variables {
            println!("  {name} = {value}");
        }
    }

    if !view.context_evolution.is_empty() {
        println!("context ({}):", view.context_evolution.len());
        for entry in &view.context_evolution {
            println!(
                "  [{}] #{} {}",
                entry.timestamp, entry.iteration, entry.description
            );
        }
    }

    if !view.status_transitions.is_empty() {
        println!("transitions ({}):", view.status_transitions.len());
        for transition in &view.status_transitions {
            println!(
                "  [{}] {} -> {}",
                transition.timestamp, transition.from, transition.to
            );
        }
    }
}
