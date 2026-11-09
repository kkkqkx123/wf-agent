use wf_types::agent::AgentTemplate;

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

use super::*;

/// Clone an agent template under a new id/name and register the clone.
/// The stored row is the source: a missing registry view is recomposed
/// from storage so storage existence alone allows the clone.
pub async fn clone_agent_template(
    ctx: &ApiContext,
    id: &str,
    new_name: &str,
) -> ApiResult<AgentTemplate> {
    let template = match get_agent_template(ctx, id) {
        Ok(template) => template,
        Err(_) => {
            let definition = load_definition(ctx, id).await?;
            library_entry(&ctx.registries, &definition)
        }
    };
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
