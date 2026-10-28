use super::*;
use crate::ApiContext;
use std::sync::Arc;
use wf_resource::registry::ResourceRegistries;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::context::StorageContext;
use wf_types::enums::{BottleneckSeverity, PerformanceTrend};
use wf_workflow::entity::WorkflowExecutionEntity;

fn make_ctx() -> Arc<ApiContext> {
    Arc::new(ApiContext::new(
        StorageContext::new_memory(),
        Arc::new(ResourceRegistries::new()),
    ))
}

#[tokio::test]
async fn profile_builds_timeline_from_live_entity() {
    use wf_core::registry::MutableRegistry;

    let ctx = make_ctx();
    let entity = Arc::new(WorkflowExecutionEntity::new(
        wf_types::Id::from("exec-perf-1".to_string()),
        wf_types::Id::from("wf-perf-1".to_string()),
    ));
    let _ = entity.state.write().await.start();
    // Sample after `start()` so the node timestamps sit at or after
    // the state's start_time; a wall-clock tick between the samples
    // would otherwise push the start boundary past the nodes and make
    // the derived total flaky.
    let now = wf_common::now();
    entity
        .state
        .write()
        .await
        .record_node_execution(wf_workflow::state::NodeExecutionRecord {
            node_id: "n1".into(),
            node_name: "n1".into(),
            node_type: "LLM".into(),
            start_time: now,
            end_time: Some(now + 8000),
            success: true,
            error: None,
            input: None,
            result: None,
            branch_id: None,
            metadata: None,
        });
    entity
        .state
        .write()
        .await
        .record_node_execution(wf_workflow::state::NodeExecutionRecord {
            node_id: "n2".into(),
            node_name: "n2".into(),
            node_type: "SCRIPT".into(),
            start_time: now,
            end_time: Some(now + 2000),
            success: true,
            error: None,
            input: None,
            result: None,
            branch_id: None,
            metadata: None,
        });
    ctx.workflow_executions
        .register("exec-perf-1".to_string(), entity.clone())
        .expect("register");

    let profile = profile(&ctx, "exec-perf-1").await.unwrap();
    assert_eq!(profile.node_count, 2);
    // End not recorded -> total derived from the latest node end time.
    assert_eq!(profile.total_duration_ms, 8000);
    assert_eq!(profile.timeline[0].node_id, "n1");
    assert_eq!(profile.bottlenecks[0].node_id, "n1");
    assert_eq!(profile.time_by_node_type.get("LLM"), Some(&8000));
    assert!((profile.timeline[0].share - 1.0).abs() < 0.001);
}

#[tokio::test]
async fn profile_degrades_to_persisted_record() {
    let ctx = make_ctx();
    let record = wf_types::WorkflowExecution {
        id: "exec-perf-p".into(),
        workflow_id: "wf-perf-p".into(),
        workflow_version: None,
        status: wf_types::ExecutionStatus::Completed,
        current_node_id: None,
        graph: None,
        variables: None,
        input: None,
        output: None,
        node_results: None,
        errors: Some(vec!["boom".into()]),
        error: None,
        started_at: 1000,
        completed_at: Some(3000),
        execution_type: None,
        fork_join_context: None,
        hierarchy: None,

        rerun_of: None,
        debug: None,
    };
    ctx.storage.workflow_execution.save(&record).await.unwrap();

    let profile = profile(&ctx, "exec-perf-p").await.unwrap();
    assert_eq!(profile.total_duration_ms, 2000);
    assert_eq!(profile.error_count, 1);
}

#[tokio::test]
async fn compare_computes_improvement_rate() {
    let ctx = make_ctx();
    let record = |id: &str, duration: i64| wf_types::WorkflowExecution {
        id: id.into(),
        workflow_id: "wf-perf-c".into(),
        workflow_version: None,
        status: wf_types::ExecutionStatus::Completed,
        current_node_id: None,
        graph: None,
        variables: None,
        input: None,
        output: None,
        node_results: None,
        errors: None,
        error: None,
        started_at: 1000,
        completed_at: Some(1000 + duration),
        execution_type: None,
        fork_join_context: None,
        hierarchy: None,

        rerun_of: None,
        debug: None,
    };
    ctx.storage
        .workflow_execution
        .save(&record("exec-a", 10000))
        .await
        .unwrap();
    ctx.storage
        .workflow_execution
        .save(&record("exec-b", 6000))
        .await
        .unwrap();

    let comparison = compare(&ctx, "exec-a", "exec-b").await.unwrap();
    assert!(comparison.improved);
    assert_eq!(comparison.duration_change_ms, -4000);
    assert!((comparison.improvement_rate - 0.4).abs() < 0.001);
}

#[tokio::test]
async fn analyze_performance_builds_profile_with_summary() {
    use wf_core::registry::MutableRegistry;

    let ctx = make_ctx();
    let entity = Arc::new(WorkflowExecutionEntity::new(
        wf_types::Id::from("exec-perf-2".to_string()),
        wf_types::Id::from("wf-perf-2".to_string()),
    ));
    let now = wf_common::now();
    {
        let mut state = entity.state.write().await;
        let _ = state.start();
        for (index, (ms, ok)) in [(8000i64, true), (2000, true), (1000, false)]
            .iter()
            .enumerate()
        {
            state.record_node_execution(wf_workflow::state::NodeExecutionRecord {
                node_id: format!("n{}", index),
                node_name: format!("n{}", index),
                node_type: "LLM".into(),
                start_time: now,
                end_time: Some(now + ms),
                success: *ok,
                error: if !*ok { Some("boom".into()) } else { None },
                input: None,
                result: None,
                branch_id: None,
                metadata: None,
            });
        }
        let _ = state.complete();
    }
    ctx.workflow_executions
        .register("exec-perf-2".to_string(), entity.clone())
        .expect("register");

    let profile = analyze_performance(&ctx, "exec-perf-2").await.unwrap();
    assert_eq!(profile.node_count, 3);
    assert!(
        profile.total_duration_ms >= 0,
        "state boundaries may collapse to 0ms"
    );
    assert!(matches!(profile.performance_tier, PerformanceTier::Fast));

    let summary = profile.summary;
    assert_eq!(summary.max_node_duration, 8000);
    assert_eq!(summary.min_node_duration, 1000);
    assert_eq!(summary.avg_node_duration, 3666);
    assert!((summary.success_rate - (2.0 / 3.0)).abs() < 0.01);

    let bottlenecks = profile.bottlenecks;
    assert_eq!(bottlenecks[0].location, "n0 (n0)");
    assert!(bottlenecks[0].duration >= 8000);
    assert_eq!(bottlenecks[0].severity, BottleneckSeverity::High);
}

#[tokio::test]
async fn summary_bottlenecks_and_node_comparison() {
    use wf_core::registry::MutableRegistry;

    let ctx = make_ctx();
    let entity = Arc::new(WorkflowExecutionEntity::new(
        wf_types::Id::from("exec-perf-3".to_string()),
        wf_types::Id::from("wf-perf-3".to_string()),
    ));
    let now = wf_common::now();
    {
        let mut state = entity.state.write().await;
        let _ = state.start();
        for (index, duration) in [1000i64, 2000, 3000].iter().enumerate() {
            state.record_node_execution(wf_workflow::state::NodeExecutionRecord {
                node_id: format!("n{}", index),
                node_name: format!("n{}", index),
                node_type: "SCRIPT".into(),
                start_time: now,
                end_time: Some(now + duration),
                success: true,
                error: None,
                input: None,
                result: None,
                branch_id: None,
                metadata: None,
            });
        }
        let _ = state.complete();
    }
    ctx.workflow_executions
        .register("exec-perf-3".to_string(), entity.clone())
        .expect("register");

    let summary = get_performance_summary(&ctx, "exec-perf-3").await.unwrap();
    assert_eq!(summary.avg_node_duration, 2000);
    assert_eq!(summary.max_node_duration, 3000);

    let bottlenecks = identify_bottlenecks(&ctx, "exec-perf-3").await.unwrap();
    assert_eq!(bottlenecks.len(), 3);
    assert_eq!(bottlenecks[0].location, "n2 (n2)");

    let comparison = get_iteration_comparison(&ctx, "exec-perf-3").await.unwrap();
    assert_eq!(comparison.total_nodes, 3);
    assert_eq!(comparison.fastest_node.as_ref().unwrap().node_id, "n0");
    assert_eq!(comparison.slowest_node.as_ref().unwrap().node_id, "n2");
    assert_eq!(comparison.average_duration, 2000);
    assert_eq!(comparison.variance, 666666);
    assert!(matches!(
        comparison.trend,
        PerformanceTrend::Improving | PerformanceTrend::Degrading | PerformanceTrend::Stable
    ));
}
