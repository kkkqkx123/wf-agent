use super::*;
use crate::ApiContext;
use crate::ApiError;
use std::sync::Arc;
use wf_core::registry::Registry;
use wf_resource::registry::{register_item_skip, ResourceRegistries};
use wf_storage::adapter::BaseStorageAdapter;
use wf_storage::context::StorageContext;
use wf_types::agent::{AgentDefinition, AgentMetadata, AgentTemplate};
use wf_types::workflow::{WorkflowDefinition, WorkflowMetadata, WorkflowTemplate};

fn now() -> i64 {
    wf_common::now()
}

fn workflow_template(id: &str, category: &str) -> WorkflowTemplate {
    WorkflowTemplate {
        id: id.into(),
        name: format!("Workflow {}", id),
        description: format!("Description {}", id),
        definition: WorkflowDefinition {
            id: id.into(),
            name: format!("Workflow {}", id),
            description: None,
            r#type: None,
            version: Some("1.0.0".into()),
            nodes: vec![
                wf_types::node::BaseStaticNode {
                    id: "start".into(),
                    node_type: wf_types::node::StaticNodeType::Start,
                    name: Some("start".into()),
                    description: None,
                    config: None,
                    execution_config: None,
                },
                wf_types::node::BaseStaticNode {
                    id: "end".into(),
                    node_type: wf_types::node::StaticNodeType::End,
                    name: Some("end".into()),
                    description: None,
                    config: None,
                    execution_config: None,
                },
            ],
            edges: vec![wf_types::workflow::Edge {
                id: "e1".into(),
                source_node_id: "start".into(),
                target_node_id: "end".into(),
                r#type: wf_types::workflow::EdgeType::Default,
                condition: None,
                label: None,
                description: None,
                weight: None,
                metadata: None,
                error_route: None,
            }],
            config: None,
            variables: None,
            triggered_subworkflow_config: None,
            metadata: Some(WorkflowMetadata {
                author: Some("system".into()),
                tags: Some(vec!["tag-a".into()]),
                category: Some(category.into()),
                ..Default::default()
            }),
            available_tools: None,
            hooks: None,
            layout: None,
            created_at: now(),
            updated_at: now(),
        },
        template_category: Some(category.into()),
        template_tags: Some(vec!["tag-a".into()]),
        is_public: Some(true),
        enabled: Some(true),
        is_system: Some(false),
        is_published: Some(true),
    }
}

fn agent_template(id: &str, category: &str) -> AgentTemplate {
    AgentTemplate {
        id: id.into(),
        name: format!("Agent {}", id),
        description: format!("Description {}", id),
        definition: AgentDefinition {
            id: id.into(),
            name: format!("Agent {}", id),
            description: None,
            version: None,
            config: None,
            metadata: Some(AgentMetadata {
                author: Some("author-x".into()),
                tags: Some(vec!["tag-b".into()]),
                category: Some(category.into()),
            }),
            created_at: now(),
            updated_at: now(),
        },
        template_category: Some(category.into()),
        template_tags: Some(vec!["tag-b".into()]),
        is_public: Some(true),
        enabled: Some(true),
        is_system: Some(false),
    }
}

fn make_ctx() -> Arc<ApiContext> {
    let registries = Arc::new(ResourceRegistries::new());
    register_item_skip(
        &registries.workflows,
        "wf-a".into(),
        workflow_template("wf-a", "analytics"),
    );
    register_item_skip(
        &registries.workflows,
        "wf-b".into(),
        workflow_template("wf-b", "writing"),
    );
    register_item_skip(
        &registries.agent_templates,
        "agent-a".into(),
        agent_template("agent-a", "analytics"),
    );
    Arc::new(ApiContext::new(StorageContext::new_memory(), registries))
}

#[tokio::test]
async fn query_filters_and_kind() {
    let ctx = make_ctx();

    let all = query(&ctx, &TemplateFilter::default()).await.unwrap();
    assert_eq!(all.len(), 3);

    let by_category = query_by_category(&ctx, "analytics").await.unwrap();
    assert_eq!(by_category.len(), 2);
    assert!(by_category.iter().all(|t| t.kind != "writing"));

    let by_author = query_by_author(&ctx, "author-x").await.unwrap();
    assert_eq!(by_author.len(), 1);
    assert_eq!(by_author[0].kind, "agent");

    let by_tags = query_by_tags(&ctx, &["tag-b".to_string()]).await.unwrap();
    assert_eq!(by_tags.len(), 1);

    let by_name = query(
        &ctx,
        &TemplateFilter {
            name: Some("wf".into()),
            ..TemplateFilter::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(by_name.len(), 2);

    let workflows_only = query(
        &ctx,
        &TemplateFilter {
            kind: Some(TemplateKind::Workflow),
            ..TemplateFilter::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(workflows_only.len(), 2);
}

#[tokio::test]
async fn featured_and_usage_tracking() {
    let ctx = make_ctx();

    record_usage(&ctx, "wf-b").await.unwrap();
    record_usage(&ctx, "wf-b").await.unwrap();
    record_usage(&ctx, "wf-a").await.unwrap();

    let featured = featured(&ctx, Some(10)).await.unwrap();
    assert_eq!(featured[0].id, "wf-b");
    assert_eq!(featured[0].usage_count, 2);

    let popular = popular_in_category(&ctx, "analytics", Some(10))
        .await
        .unwrap();
    assert_eq!(popular.len(), 2);
    assert_eq!(popular[0].id, "wf-a");
}

#[tokio::test]
async fn usage_survives_context_rebuild() {
    let storage = StorageContext::new_memory();
    let registries = Arc::new(ResourceRegistries::new());
    register_item_skip(
        &registries.workflows,
        "wf-a".into(),
        workflow_template("wf-a", "analytics"),
    );
    let ctx = Arc::new(ApiContext::new(storage, registries));
    record_usage(&ctx, "wf-a").await.unwrap();
    assert_eq!(usage_count(&ctx, "wf-a").await, 1);
    assert_eq!(usage_count(&ctx, "missing").await, 0);
}

#[tokio::test]
async fn update_replaces_in_place_and_rejects_missing() {
    let ctx = make_ctx();

    let mut renamed = workflow_template("wf-a", "analytics");
    renamed.name = "Renamed".into();
    update_workflow_template(&ctx, &renamed).await.unwrap();
    assert_eq!(get_workflow_template(&ctx, "wf-a").unwrap().name, "Renamed");
    let missing = workflow_template("nope", "analytics");
    assert!(matches!(
        update_workflow_template(&ctx, &missing).await.unwrap_err(),
        ApiError::NotFound { .. }
    ));

    let mut agent_renamed = agent_template("agent-a", "analytics");
    agent_renamed.name = "Renamed Agent".into();
    update_agent_template(&ctx, &agent_renamed).await.unwrap();
    assert_eq!(
        get_agent_template(&ctx, "agent-a").unwrap().name,
        "Renamed Agent"
    );
    let agent_missing = agent_template("nope", "analytics");
    assert!(matches!(
        update_agent_template(&ctx, &agent_missing)
            .await
            .unwrap_err(),
        ApiError::NotFound { .. }
    ));
}

#[tokio::test]
async fn clone_registers_and_get() {
    let ctx = make_ctx();
    register_workflow_template(&ctx, &workflow_template("wf-clone-src", "analytics"))
        .await
        .unwrap();

    let cloned_id = crate::workflow::clone_workflow(&ctx, "wf-clone-src", None)
        .await
        .unwrap();
    assert_ne!(cloned_id, "wf-clone-src");
    assert!(ctx.registries.workflows.has(&cloned_id));

    let cloned_agent = clone_agent_template(&ctx, "agent-a", "Agent Clone")
        .await
        .unwrap();
    assert!(ctx.registries.agent_templates.has(&cloned_agent.id));

    let err = get_workflow_template(&ctx, "missing").unwrap_err();
    assert!(matches!(err, ApiError::NotFound { .. }));
}

/// The published state belongs to the definition, so it survives a
/// restart: the stored definition is what the next start reads back.
#[tokio::test]
async fn publish_state_travels_with_the_definition() {
    let ctx = make_ctx();
    let template = workflow_template("wf-publish", "analytics");
    register_workflow_template(&ctx, &template).await.unwrap();

    let stored = ctx
        .storage
        .workflow
        .load("wf-publish")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.metadata.as_ref().and_then(|m| m.published),
        Some(true)
    );
    assert_eq!(list_workflow_templates(&ctx).unwrap().len(), 3);

    unpublish_workflow_template(&ctx, "wf-publish")
        .await
        .unwrap();
    let stored = ctx
        .storage
        .workflow
        .load("wf-publish")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.metadata.as_ref().and_then(|m| m.published),
        Some(false)
    );
    assert_eq!(list_workflow_templates(&ctx).unwrap().len(), 3);

    publish_workflow_template(&ctx, "wf-publish").await.unwrap();
    let stored = ctx
        .storage
        .workflow
        .load("wf-publish")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.metadata.as_ref().and_then(|m| m.published),
        Some(true)
    );
    assert_eq!(list_workflow_templates(&ctx).unwrap().len(), 3);
}

/// Removing a template from the library leaves the workflow in place:
/// the published mark is cleared and the definition stays.
#[tokio::test]
async fn delete_keeps_the_workflow_and_clears_the_published_mark() {
    let ctx = make_ctx();
    let template = workflow_template("wf-delete", "analytics");
    register_workflow_template(&ctx, &template).await.unwrap();

    delete_workflow_template(&ctx, "wf-delete").await.unwrap();

    assert!(!ctx.registries.workflows.has("wf-delete"));
    let stored = ctx
        .storage
        .workflow
        .load("wf-delete")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.metadata.as_ref().and_then(|m| m.published),
        Some(false)
    );
    assert_eq!(list_workflow_templates(&ctx).unwrap().len(), 2);
}

/// A workflow taken out of the library can be published again: the
/// publish mark lives in the definition, so the entry is rebuilt from it.
#[tokio::test]
async fn a_workflow_taken_out_of_the_library_can_be_published_again() {
    let ctx = make_ctx();
    let template = workflow_template("wf-republish", "analytics");
    register_workflow_template(&ctx, &template).await.unwrap();
    delete_workflow_template(&ctx, "wf-republish")
        .await
        .unwrap();
    assert!(!ctx.registries.workflows.has("wf-republish"));

    publish_workflow_template(&ctx, "wf-republish")
        .await
        .unwrap();

    assert!(ctx.registries.workflows.has("wf-republish"));
    let stored = ctx
        .storage
        .workflow
        .load("wf-republish")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.metadata.as_ref().and_then(|m| m.published),
        Some(true)
    );
    assert_eq!(list_workflow_templates(&ctx).unwrap().len(), 3);
}

/// The library governance travels with the definition, so it outlives a
/// restart: the entry rebuilt from the stored definition carries it.
#[tokio::test]
async fn governance_travels_with_the_definition() {
    let ctx = make_ctx();
    let mut template = workflow_template("wf-governed", "analytics");
    template.is_public = Some(false);
    template.is_system = Some(true);
    template.enabled = Some(false);
    register_workflow_template(&ctx, &template).await.unwrap();

    let stored = ctx
        .storage
        .workflow
        .load("wf-governed")
        .await
        .unwrap()
        .unwrap();
    let metadata = stored.metadata.as_ref().unwrap();
    assert_eq!(metadata.is_public, Some(false));
    assert_eq!(metadata.is_system, Some(true));
    assert_eq!(metadata.enabled, Some(false));

    let registries = Arc::new(ResourceRegistries::new());
    crate::workflow::upsert_workflow_registry(&registries, &stored).unwrap();
    let entry = registries.workflows.get("wf-governed").unwrap();
    assert_eq!(entry.is_public, Some(false));
    assert_eq!(entry.is_system, Some(true));
    assert_eq!(entry.enabled, Some(false));
    assert_eq!(entry.is_published, Some(true));
}
