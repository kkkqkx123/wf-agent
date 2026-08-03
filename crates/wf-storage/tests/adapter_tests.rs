#![allow(clippy::manual_is_multiple_of)]

use wf_storage::adapter::adapter_impls::*;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::*;
use wf_storage::store::memory::MemoryStorage;

fn make_store(name: &str) -> MemoryStorage {
    MemoryStorage::new(name)
}

fn make_wf(i: u32) -> wf_types::WorkflowDefinition {
    use wf_types::workflow::WorkflowDefinitionType;
    wf_types::WorkflowDefinition {
        id: format!("wf-{}", i),
        name: format!("wf_{}", i),
        description: None,
        r#type: Some(if i % 2 == 0 {
            WorkflowDefinitionType::Standalone
        } else {
            WorkflowDefinitionType::Dependent
        }),
        version: None,
        nodes: vec![],
        edges: vec![],
        config: None,
        variables: None,
        triggers: None,
        triggered_subworkflow_config: None,
        metadata: None,
        created_at: 1000,
        updated_at: 1000,
        available_tools: None,
    }
}

fn make_exec(i: u32) -> wf_types::WorkflowExecution {
    wf_types::WorkflowExecution {
        id: format!("ex-{}", i),
        workflow_id: format!("wf-{}", i),
        workflow_version: None,
        status: wf_types::ExecutionStatus::Created,
        current_node_id: None,
        graph: None,
        variables: None,
        input: None,
        output: None,
        node_results: None,
        errors: None,
        started_at: 1000,
        completed_at: None,
        error: None,
        execution_type: None,
        fork_join_context: None,
        hierarchy: None,
    }
}

#[tokio::test]
async fn test_workflow_adapter_crud() {
    let adapter = WorkflowStorage::new(make_store("test_wf"));
    let wf = make_wf(1);
    adapter.save(&wf).await.unwrap();
    assert!(adapter.exists("wf-1").await.unwrap());
    let loaded = adapter.load("wf-1").await.unwrap().unwrap();
    assert_eq!(loaded.name, "wf_1");
    assert!(adapter.delete("wf-1").await.unwrap());
    assert!(!adapter.exists("wf-1").await.unwrap());
}

#[tokio::test]
async fn test_trigger_adapter_list_by_event() {
    let adapter = TriggerStorage::new(make_store("test_tr"));
    let t1 = wf_types::TriggerStorageMetadata {
        id: "tr-1".into(),
        name: "t1".into(),
        description: None,
        event: "pull_request".into(),
        enabled: true,
        created_at: 1000,
        updated_at: 1000,
    };
    let t2 = wf_types::TriggerStorageMetadata {
        id: "tr-2".into(),
        name: "t2".into(),
        description: None,
        event: "push".into(),
        enabled: true,
        created_at: 1001,
        updated_at: 1001,
    };
    adapter.save(&t1).await.unwrap();
    adapter.save(&t2).await.unwrap();
    let pr_triggers = adapter.list_by_event("pull_request").await.unwrap();
    assert_eq!(pr_triggers.len(), 1);
    assert_eq!(pr_triggers[0].name, "t1");
}

#[tokio::test]
async fn test_tool_adapter_get_stats() {
    let adapter = ToolStorage::new(make_store("test_tl"));
    for i in 0..3 {
        adapter
            .save(&wf_types::ToolStorageMetadata {
                id: format!("tl-{}", i),
                tool_id: format!("tool_{}", i),
                tool_type: "builtin".into(),
                description: None,
                enabled: true,
                created_at: 1000,
                updated_at: 1000,
            })
            .await
            .unwrap();
    }
    adapter
        .save(&wf_types::ToolStorageMetadata {
            id: "tl-ext".into(),
            tool_id: "ext_1".into(),
            tool_type: "mcp".into(),
            description: None,
            enabled: true,
            created_at: 1001,
            updated_at: 1001,
        })
        .await
        .unwrap();
    let stats = adapter.get_stats().await.unwrap();
    assert_eq!(*stats.get("builtin").unwrap(), 3);
    assert_eq!(*stats.get("mcp").unwrap(), 1);
}

#[tokio::test]
async fn test_script_adapter_list_by_language() {
    let adapter = ScriptStorage::new(make_store("test_sc"));
    adapter
        .save(&wf_types::ScriptStorageMetadata {
            id: "sc-1".into(),
            name: "s1".into(),
            description: None,
            language: Some("python".into()),
            enabled: true,
            created_at: 1000,
            updated_at: 1000,
        })
        .await
        .unwrap();
    adapter
        .save(&wf_types::ScriptStorageMetadata {
            id: "sc-2".into(),
            name: "s2".into(),
            description: None,
            language: Some("javascript".into()),
            enabled: true,
            created_at: 1001,
            updated_at: 1001,
        })
        .await
        .unwrap();
    let py = adapter.list_by_language("python").await.unwrap();
    assert_eq!(py.len(), 1);
}

#[tokio::test]
async fn test_node_template_adapter_list_by_node_type() {
    let adapter = NodeTemplateStorage::new(make_store("test_nt"));
    adapter
        .save(&wf_types::NodeTemplateStorageMetadata {
            id: "nt-1".into(),
            name: "nt1".into(),
            node_type: "llm".into(),
            description: None,
            created_at: 1000,
            updated_at: 1000,
        })
        .await
        .unwrap();
    adapter
        .save(&wf_types::NodeTemplateStorageMetadata {
            id: "nt-2".into(),
            name: "nt2".into(),
            node_type: "code".into(),
            description: None,
            created_at: 1001,
            updated_at: 1001,
        })
        .await
        .unwrap();
    let llm = adapter.list_by_node_type("llm").await.unwrap();
    assert_eq!(llm.len(), 1);
}

#[tokio::test]
async fn test_hook_template_adapter_list_by_hook_type() {
    let adapter = HookTemplateStorage::new(make_store("test_ht"));
    adapter
        .save(&wf_types::HookTemplateStorageMetadata {
            id: "ht-1".into(),
            name: "ht1".into(),
            hook_type: "before_execute".into(),
            description: None,
            created_at: 1000,
            updated_at: 1000,
        })
        .await
        .unwrap();
    let before = adapter.list_by_hook_type("before_execute").await.unwrap();
    assert_eq!(before.len(), 1);
}

#[tokio::test]
async fn test_agent_profile_adapter_get_first() {
    let adapter = AgentProfileStorage::new(make_store("test_ap"));
    assert!(adapter.get_first().await.unwrap().is_none());
    adapter
        .save(&wf_types::AgentProfileStorageMetadata {
            id: "ap-1".into(),
            profile_id: "default".into(),
            name: "Default Agent".into(),
            description: None,
            created_at: 1000,
            updated_at: 1000,
        })
        .await
        .unwrap();
    let first = adapter.get_first().await.unwrap().unwrap();
    assert_eq!(first.name, "Default Agent");
}

#[tokio::test]
async fn test_execution_adapter_update_status() {
    let adapter = WorkflowExecutionStorage::new(make_store("test_ex"));
    let exec = make_exec(1);
    adapter.save(&exec).await.unwrap();
    adapter
        .update_status("ex-1", &wf_types::ExecutionStatus::Running)
        .await
        .unwrap();
    let loaded = adapter.load("ex-1").await.unwrap().unwrap();
    assert_eq!(loaded.status, wf_types::ExecutionStatus::Running);
}

#[tokio::test]
async fn test_file_checkpoint_adapter_load_by_path() {
    let adapter = FileCheckpointStorage::new(make_store("test_fc"));
    adapter
        .save(&wf_types::FileCheckpointStorageMetadata {
            id: "fc-1".into(),
            entity_id: "entity-1".into(),
            file_path: "/tmp/test.txt".into(),
            checkpoint_id: "cp-1".into(),
            size_bytes: 100,
            compressed: false,
            created_at: 1000,
        })
        .await
        .unwrap();
    let found = adapter
        .load_by_file_path("/tmp/test.txt")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.id, "fc-1");
}

#[tokio::test]
async fn test_metrics_adapter_save_query_delete() {
    let adapter = MetricsStorage::new(make_store("test_mt"));
    let points = vec![
        wf_storage::adapter::MetricsDataPoint {
            name: "cpu".into(),
            metric_type: "gauge".into(),
            value: 0.8,
            timestamp: 2000,
            tags: None,
        },
        wf_storage::adapter::MetricsDataPoint {
            name: "cpu".into(),
            metric_type: "gauge".into(),
            value: 0.5,
            timestamp: 1000,
            tags: None,
        },
    ];
    adapter.save_batch(&points).await.unwrap();
    let results = adapter.query("cpu", 500, 1500).await.unwrap();
    assert_eq!(results.len(), 1);
    assert!((results[0].value - 0.5).abs() < 1e-10);
    let all = adapter.query("cpu", 0, 3000).await.unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].timestamp, 1000);
    assert_eq!(all[1].timestamp, 2000);
    adapter.delete_old(1500).await.unwrap();
    let after = adapter.query("cpu", 0, 3000).await.unwrap();
    assert_eq!(after.len(), 1);
}

#[tokio::test]
async fn test_list_options_filtering() {
    let adapter = WorkflowStorage::new(make_store("test_list"));
    for i in 0..5 {
        adapter.save(&make_wf(i)).await.unwrap();
    }
    let all = adapter.list(None).await.unwrap();
    assert_eq!(all.len(), 5);
    let opts = WorkflowListOptions {
        offset: None,
        limit: None,
        name_filter: None,
        type_filter: Some("STANDALONE".into()),
    };
    let filtered = adapter.list(Some(opts)).await.unwrap();
    assert_eq!(filtered.len(), 3);
}

#[tokio::test]
async fn test_workflow_versions() {
    let adapter = WorkflowStorage::new(make_store("test_versions"));
    let wf1 = make_wf(1);
    adapter.save_version("wf-1", "1", &wf1).await.unwrap();
    adapter.save_version("wf-1", "2", &wf1).await.unwrap();
    adapter.save_version("wf-2", "1", &wf1).await.unwrap();

    let versions = adapter.list_versions("wf-1").await.unwrap();
    assert_eq!(versions.len(), 2);
    assert_eq!(
        adapter.load_version("wf-1", "1").await.unwrap().unwrap().id,
        "wf-1"
    );
    assert!(adapter.delete_version("wf-1", "1").await.unwrap());
    assert!(!adapter.delete_version("wf-1", "1").await.unwrap());
    assert_eq!(adapter.list_versions("wf-1").await.unwrap().len(), 1);
}

#[tokio::test]
async fn test_checkpoint_latest_by_entity() {
    let adapter = CheckpointStorage::new(make_store("test_cp_latest"));
    for (i, ts) in [(1, 1000), (2, 2000), (3, 1500)] {
        adapter
            .save(&wf_types::Checkpoint {
                id: format!("cp-{}", i),
                entity_type: "execution".into(),
                entity_id: "ex-1".into(),
                checkpoint_type: wf_types::checkpoint::base::CheckpointType::Full,
                timestamp: ts,
                status: wf_types::checkpoint::base::CheckpointStatus::Active,
                previous_checkpoint_id: None,
                base_checkpoint_id: None,
                chain_root_id: None,
                chain_position: None,
                blob_size: None,
                tags: None,
                custom_fields: None,
            })
            .await
            .unwrap();
    }
    let latest = adapter
        .get_latest_by_entity("ex-1", "checkpoint")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(latest.id, "cp-2");
}

#[tokio::test]
async fn test_get_stats_pushdown() {
    let adapter = AgentLoopStorage::new(make_store("test_loop_stats"));
    for i in 0..3 {
        adapter
            .save(&wf_types::AgentLoopStorageMetadata {
                id: format!("loop-{}", i),
                definition_id: "def-1".into(),
                status: "running".into(),
                current_iteration: 0,
                started_at: 1000,
                updated_at: 1000,
            })
            .await
            .unwrap();
    }
    adapter
        .save(&wf_types::AgentLoopStorageMetadata {
            id: "loop-done".into(),
            definition_id: "def-1".into(),
            status: "completed".into(),
            current_iteration: 3,
            started_at: 1000,
            updated_at: 1000,
        })
        .await
        .unwrap();
    let stats = adapter.get_stats().await.unwrap();
    assert_eq!(*stats.get("running").unwrap(), 3);
    assert_eq!(*stats.get("completed").unwrap(), 1);
}
