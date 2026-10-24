use wf_types::agent::AgentTemplate;
use wf_types::workflow::WorkflowTemplate;

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

use super::*;

pub async fn clone_workflow_template(
    ctx: &ApiContext,
    id: &str,
    new_name: &str,
) -> ApiResult<WorkflowTemplate> {
    let template = get_workflow_template(ctx, id)?;
    let mut cloned = template.clone();
    cloned.id = format!("cloned-{}", wf_common::generate_id());
    cloned.name = new_name.to_string();
    cloned.description = if template.description.is_empty() {
        format!("Clone of {}", template.name)
    } else {
        format!("Clone of {}", template.description)
    };
    cloned.definition.id = cloned.id.clone();
    cloned.definition.name = new_name.to_string();
    cloned.definition.created_at = wf_common::now();
    cloned.definition.updated_at = wf_common::now();
    cloned
        .definition
        .metadata
        .get_or_insert_with(Default::default)
        .derived_from = Some(wf_types::workflow::definition::DefinitionDerivation::new(
        "workflow",
        id,
        wf_common::now(),
    ));
    register_workflow_template(ctx, &cloned).await?;
    Ok(cloned)
}

/// Clone an agent template under a new id/name and register the clone.
pub async fn clone_agent_template(
    ctx: &ApiContext,
    id: &str,
    new_name: &str,
) -> ApiResult<AgentTemplate> {
    let template = get_agent_template(ctx, id)?;
    let mut cloned = template.clone();
    cloned.id = format!("cloned-{}", wf_common::generate_id());
    cloned.name = new_name.to_string();
    cloned.description = if template.description.is_empty() {
        format!("Clone of {}", template.name)
    } else {
        format!("Clone of {}", template.description)
    };
    cloned.definition.id = cloned.id.clone();
    cloned.definition.name = new_name.to_string();
    cloned.definition.created_at = wf_common::now();
    cloned.definition.updated_at = wf_common::now();
    register_agent_template(ctx, &cloned).await?;
    Ok(cloned)
}
