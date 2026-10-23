use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::context::StorageContext;
use wf_types::node::StaticNodeType;

pub struct ScriptReference {
    pub workflow_id: String,
    pub workflow_name: String,
    pub node_id: String,
}

/// Check whether any stored workflow references the script (by name or id)
/// from a SCRIPT / INTERACTIVE_SCRIPT node. The report is used before
/// deletion to avoid orphaning workflows.
pub async fn check_script_delete_references(
    ctx: &StorageContext,
    id: &str,
) -> crate::ApiResult<Vec<ScriptReference>> {
    let script = match ctx.script.load(id).await? {
        Some(script) => script,
        None => return Ok(Vec::new()),
    };
    let candidates = [script.id.clone(), script.name.clone()];

    let is_script_node = |node: &wf_types::node::BaseStaticNode| {
        matches!(
            node.node_type,
            StaticNodeType::Script | StaticNodeType::InteractiveScript
        )
    };
    let mut references = Vec::new();
    for (workflow_id, workflow_name, node_id) in crate::infra::reference::collect_node_references(
        ctx,
        is_script_node,
        &["script_name", "scriptName"],
        &candidates,
    )
    .await?
    {
        references.push(ScriptReference {
            workflow_id,
            workflow_name,
            node_id,
        });
    }
    Ok(references)
}
