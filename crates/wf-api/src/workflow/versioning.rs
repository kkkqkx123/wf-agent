//! Semantic versioning and versioned workflow updates.

use wf_types::workflow::WorkflowMetadata;

use crate::ApiContext;

/// Semantic version increment strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionStrategy {
    Patch,
    Minor,
    Major,
}

/// Field-level changes applied by versioned update.
#[derive(Debug, Clone, Default)]
pub struct WorkflowChanges {
    pub name: Option<String>,
    pub description: Option<Option<String>>,
    pub metadata: Option<WorkflowMetadata>,
    pub nodes: Option<Vec<wf_types::node::BaseStaticNode>>,
    pub edges: Option<Vec<wf_types::workflow::Edge>>,
    pub version: Option<String>,
}

/// Create a versioned update of a workflow. Returns the new version.
pub async fn create_versioned_update(
    ctx: &ApiContext,
    workflow_id: &str,
    strategy: VersionStrategy,
    changes: &WorkflowChanges,
    keep_original: bool,
) -> crate::ApiResult<String> {
    let current = super::definition::get_workflow(ctx, workflow_id).await?;

    if keep_original {
        let label = current
            .version
            .clone()
            .unwrap_or_else(|| "pre-update".to_string());
        super::version::save_workflow_version(ctx, workflow_id, &label, &current).await?;
    }

    let mut updated = current.clone();
    if let Some(name) = &changes.name {
        updated.name = name.clone();
    }
    if let Some(description) = &changes.description {
        updated.description = description.clone();
    }
    if let Some(metadata) = &changes.metadata {
        updated.metadata = Some(metadata.clone());
    }
    if let Some(nodes) = &changes.nodes {
        updated.nodes = nodes.clone();
    }
    if let Some(edges) = &changes.edges {
        updated.edges = edges.clone();
    }
    let new_version = match &changes.version {
        Some(version) => version.clone(),
        None => auto_increment_version(current.version.as_deref().unwrap_or("0.0.0"), strategy),
    };
    updated.version = Some(new_version.clone());
    updated.updated_at = wf_common::now();

    super::definition::save_workflow(ctx, &updated).await?;
    Ok(new_version)
}

/// Auto-increment a semver string.
pub fn auto_increment_version(current_version: &str, strategy: VersionStrategy) -> String {
    let parts: Vec<u32> = current_version
        .split('.')
        .filter_map(|p| p.parse().ok())
        .collect();
    let mut major = parts.first().copied().unwrap_or(0);
    let mut minor = parts.get(1).copied().unwrap_or(0);
    let mut patch = parts.get(2).copied().unwrap_or(0);
    match strategy {
        VersionStrategy::Major => {
            major += 1;
            minor = 0;
            patch = 0;
        }
        VersionStrategy::Minor => {
            minor += 1;
            patch = 0;
        }
        VersionStrategy::Patch => {
            patch += 1;
        }
    }
    format!("{major}.{minor}.{patch}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::definition::{get_workflow, save_workflow};
    use crate::workflow::version::{get_workflow_version as gv, list_workflow_versions};
    use std::sync::Arc;
    use wf_common;
    use wf_core::registry::Registry;
    use wf_resource::registry::ResourceRegistries;
    use wf_resource::resource_plugin::ResourcePluginRegistry;
    use wf_storage::context::StorageContext;
    use wf_types::WorkflowDefinition;

    fn make_ctx() -> ApiContext {
        let storage = StorageContext::new_memory();
        let registries = Arc::new(ResourceRegistries::new());
        let bundles = Arc::new(ResourcePluginRegistry::new());
        ApiContext::new(storage, registries, bundles)
    }

    fn make_workflow(id: &str) -> WorkflowDefinition {
        WorkflowDefinition {
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
            }],
            config: None,
            variables: None,
            triggered_subworkflow_config: None,
            metadata: None,
            available_tools: None,
            hooks: None,
            created_at: wf_common::now(),
            updated_at: wf_common::now(),
        }
    }

    #[tokio::test]
    async fn test_auto_increment_version() {
        assert_eq!(
            auto_increment_version("1.2.3", VersionStrategy::Patch),
            "1.2.4"
        );
        assert_eq!(
            auto_increment_version("1.2.3", VersionStrategy::Minor),
            "1.3.0"
        );
        assert_eq!(
            auto_increment_version("1.2.3", VersionStrategy::Major),
            "2.0.0"
        );
        assert_eq!(auto_increment_version("", VersionStrategy::Patch), "0.0.1");
        assert_eq!(
            auto_increment_version("v2", VersionStrategy::Minor),
            "0.1.0"
        );
    }

    #[tokio::test]
    async fn test_create_versioned_update_preserves_original() {
        let ctx = make_ctx();
        save_workflow(&ctx, &make_workflow("wf-ver")).await.unwrap();

        let new_version = create_versioned_update(
            &ctx,
            "wf-ver",
            VersionStrategy::Minor,
            &WorkflowChanges {
                name: Some("Renamed".into()),
                description: Some(Some("new description".into())),
                ..Default::default()
            },
            true,
        )
        .await
        .unwrap();
        assert_eq!(new_version, "1.1.0");

        let updated = get_workflow(&ctx, "wf-ver").await.unwrap();
        assert_eq!(updated.name, "Renamed");
        assert_eq!(updated.version.as_deref(), Some("1.1.0"));
        assert_eq!(
            ctx.registries.workflows.get("wf-ver").unwrap().name,
            "Renamed"
        );

        let original = gv(&ctx, "wf-ver", "1.0.0")
            .await
            .expect("original preserved as version");
        assert_eq!(original.name, "Workflow wf-ver");
    }

    #[tokio::test]
    async fn test_create_versioned_update_without_keeping_original() {
        let ctx = make_ctx();
        save_workflow(&ctx, &make_workflow("wf-ver2"))
            .await
            .unwrap();

        let new_version = create_versioned_update(
            &ctx,
            "wf-ver2",
            VersionStrategy::Major,
            &WorkflowChanges {
                version: Some("9.9.9".into()),
                ..Default::default()
            },
            false,
        )
        .await
        .unwrap();
        assert_eq!(new_version, "9.9.9");

        assert!(list_workflow_versions(&ctx, "wf-ver2")
            .await
            .unwrap()
            .is_empty());

        let err = create_versioned_update(
            &ctx,
            "wf-missing",
            VersionStrategy::Patch,
            &WorkflowChanges::default(),
            true,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, crate::ApiError::NotFound { .. }));
    }
}
