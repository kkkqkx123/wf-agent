//! Tests for the agent decision-graph analysis.

use std::sync::Arc;

use wf_agent::entity::AgentLoopEntity;
use wf_resource::registry::ResourceRegistries;
use wf_storage::context::StorageContext;

use super::graph::analyze;
use crate::agent::agent_graph::path_analysis::{
    all_alternatives, all_paths, alternative_decisions, analyze_decision_patterns,
    analyze_path_efficiency, critical_path, decision_edges, decision_graph, decision_nodes,
    decision_sequence, decisions_by_type, decisions_in_iteration, execution_path,
    execution_path_steps, incoming_edges, most_promising_unexplored, outgoing_edges,
    path_probability_analysis, path_statistics, unexplored_alternatives,
};
use crate::infra::context::ApiContext;

fn make_ctx() -> Arc<ApiContext> {
    Arc::new(ApiContext::new(
        StorageContext::new_memory(),
        Arc::new(ResourceRegistries::new()),
    ))
}

#[tokio::test]
async fn builds_decision_graph_from_live_entity() {
    let ctx = make_ctx();
    let entity = Arc::new(AgentLoopEntity::new(wf_types::Id::from(
        "agent-graph-1".to_string(),
    )));
    entity.state.write().await.start().unwrap();
    entity.state.write().await.start_iteration();
    entity
        .state
        .write()
        .await
        .record_tool_call("http", 10, true);
    entity.state.write().await.end_iteration();
    entity.state.write().await.start_iteration();
    entity.state.write().await.end_iteration();
    let _ = ctx.agent_loops.register(entity.clone());

    let graph = analyze(&ctx, "agent-graph-1").await.unwrap();
    assert_eq!(graph.iterations.len(), 2);
    assert_eq!(graph.iterations[0].decision, "tool:http");
    assert_eq!(graph.iterations[1].decision, "llm");
    assert_eq!(graph.tool_sequence, vec!["http"]);
    assert_eq!(graph.explored_branches, 1);
    assert_eq!(graph.path_efficiency, 0.5);
}

#[tokio::test]
async fn unknown_loop_degrades_to_empty_analysis() {
    let ctx = make_ctx();
    let analysis = analyze(&ctx, "missing").await.unwrap();
    assert!(analysis.tool_sequence.is_empty());
    assert!(analysis.explored_branches == 0);
}

#[tokio::test]
async fn decision_graph_and_path_queries() {
    let ctx = make_ctx();
    let entity = Arc::new(AgentLoopEntity::new(wf_types::Id::from(
        "agent-graph-2".to_string(),
    )));
    {
        let mut state = entity.state.write().await;
        state.start().unwrap();
        state.start_iteration();
        state.record_tool_call("http", 10, true);
        state.end_iteration();
        state.start_iteration();
        state.end_iteration();
        state.complete().unwrap();
    }
    let _ = ctx.agent_loops.register(entity.clone());

    let graph = decision_graph(&ctx, "agent-graph-2").await.unwrap();
    assert_eq!(graph.start_node_id, "start");
    assert_eq!(graph.end_node_id.as_deref(), Some("end"));
    // start + 2 decisions + 1 tool + end
    assert_eq!(graph.nodes.len(), 5);
    assert!(graph.total_paths >= 1);
    assert_eq!(graph.executed_paths, 1);
    assert!(graph.graph_density.is_some());

    let nodes = decision_nodes(&ctx, "agent-graph-2").await.unwrap();
    assert!(nodes.iter().any(|n| n.r#type == "start"));
    assert!(nodes.iter().any(|n| n.r#type == "decision"));

    let edges = decision_edges(&ctx, "agent-graph-2").await.unwrap();
    assert!(!edges.is_empty());

    let outgoing = outgoing_edges(&ctx, "agent-graph-2", "start")
        .await
        .unwrap();
    assert_eq!(outgoing.len(), 1);

    let incoming = incoming_edges(&ctx, "agent-graph-2", "end").await.unwrap();
    assert_eq!(incoming.len(), 1);

    let paths = all_paths(&ctx, "agent-graph-2").await.unwrap();
    assert!(!paths.is_empty());
    assert!(paths
        .iter()
        .all(|p| p.first() == Some(&"start".to_string())));

    let path = execution_path(&ctx, "agent-graph-2")
        .await
        .unwrap()
        .unwrap();
    assert!(path.is_successful);
    assert_eq!(path.steps.len(), 3, "2 decisions + 1 tool call");
    assert!(path.total_duration >= 0);

    let steps = execution_path_steps(&ctx, "agent-graph-2").await.unwrap();
    assert_eq!(steps.len(), 3);

    let stats = path_statistics(&ctx, "agent-graph-2")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stats.steps_count, 3);

    let critical = critical_path(&ctx, "agent-graph-2").await.unwrap().unwrap();
    assert_eq!(critical.first().map(String::as_str), Some("start"));
}

#[tokio::test]
async fn alternatives_sequence_and_probability() {
    let ctx = make_ctx();
    let entity = Arc::new(AgentLoopEntity::new(wf_types::Id::from(
        "agent-graph-3".to_string(),
    )));
    {
        let mut state = entity.state.write().await;
        state.start().unwrap();
        state.start_iteration();
        state.record_tool_call("http", 10, true);
        state.end_iteration();
        state.complete().unwrap();
    }
    let _ = ctx.agent_loops.register(entity.clone());

    // Register an unused tool so the unexplored branch analysis has data.
    ctx.tool_registry.register_tool(wf_types::tool::Tool {
        id: wf_types::Id::from("t-search".to_string()),
        name: "search".to_string(),
        description: "web search".to_string(),
        tool_type: wf_types::tool::state::ToolType::BuiltIn,
        parameters: None,
        metadata: None,
        config: None,
        enabled: None,
        strict: None,
        default_timeout_ms: None,
    });

    let alternatives = all_alternatives(&ctx, "agent-graph-3").await.unwrap();
    assert_eq!(alternatives.len(), 1);
    assert_eq!(alternatives[0].iteration, 1);
    assert_eq!(
        alternatives[0].chosen_decision.description,
        "Called tool 'http'"
    );

    let at_iter = alternative_decisions(&ctx, "agent-graph-3", 1)
        .await
        .unwrap();
    assert!(at_iter.is_some());
    assert!(alternative_decisions(&ctx, "agent-graph-3", 99)
        .await
        .unwrap()
        .is_none());

    let unexplored = unexplored_alternatives(&ctx, "agent-graph-3")
        .await
        .unwrap();
    assert!(
        !unexplored.is_empty(),
        "registry has no tools, but unused names remain derivable"
    );

    let sequence = decision_sequence(&ctx, "agent-graph-3")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(sequence.total_decisions, 1);
    assert_eq!(
        sequence
            .patterns
            .as_ref()
            .unwrap()
            .most_common_decision_type,
        "tool_selection"
    );

    let patterns = analyze_decision_patterns(&ctx, "agent-graph-3")
        .await
        .unwrap()
        .unwrap();
    assert!(patterns.consistency_score >= 0.0);

    let efficiency = analyze_path_efficiency(&ctx, "agent-graph-3")
        .await
        .unwrap()
        .unwrap();
    assert!(efficiency.executed_steps >= 1);
    assert!(efficiency.efficiency_ratio >= 1.0);

    let probability = path_probability_analysis(&ctx, "agent-graph-3")
        .await
        .unwrap()
        .unwrap();
    assert!(!probability.paths.is_empty());
    assert!(probability.most_likely_path.is_some());
    assert!(probability.paths.iter().any(|p| p.is_taken));

    let in_iteration = decisions_in_iteration(&ctx, "agent-graph-3", 1)
        .await
        .unwrap();
    assert_eq!(in_iteration.len(), 1);
    let by_type = decisions_by_type(&ctx, "agent-graph-3", "tool_selection")
        .await
        .unwrap();
    assert_eq!(by_type.len(), 1);
    assert!(most_promising_unexplored(&ctx, "agent-graph-3")
        .await
        .unwrap()
        .is_some());
}
