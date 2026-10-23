use wf_types::agent::AgentTemplate;
use wf_types::workflow::WorkflowTemplate;

use super::*;

pub(super) fn summary_from_workflow(
    template: WorkflowTemplate,
    usage_count: u64,
) -> TemplateSummary {
    let author = template
        .definition
        .metadata
        .as_ref()
        .and_then(|m| m.author.clone());
    TemplateSummary {
        id: template.id.to_string(),
        kind: TemplateKind::Workflow.as_str(),
        name: template.name,
        description: template.description,
        category: template.template_category,
        tags: template.template_tags,
        author,
        is_public: template.is_public.unwrap_or(true),
        enabled: template.enabled.unwrap_or(true),
        is_system: template.is_system.unwrap_or(false),
        usage_count,
        created_at: template.definition.created_at,
        updated_at: template.definition.updated_at,
    }
}

pub(super) fn summary_from_agent(template: AgentTemplate, usage_count: u64) -> TemplateSummary {
    let author = template
        .definition
        .metadata
        .as_ref()
        .and_then(|m| m.author.clone());
    TemplateSummary {
        id: template.id.to_string(),
        kind: TemplateKind::Agent.as_str(),
        name: template.name,
        description: template.description,
        category: template.template_category,
        tags: template.template_tags,
        author,
        is_public: template.is_public.unwrap_or(true),
        enabled: template.enabled.unwrap_or(true),
        is_system: template.is_system.unwrap_or(false),
        usage_count,
        created_at: template.definition.created_at,
        updated_at: template.definition.updated_at,
    }
}

pub(super) fn summary_from_node(
    template: wf_types::NodeTemplateStorageMetadata,
    usage_count: u64,
) -> TemplateSummary {
    TemplateSummary {
        id: template.id.to_string(),
        kind: TemplateKind::Node.as_str(),
        name: template.name,
        description: template.description.unwrap_or_default(),
        category: None,
        tags: None,
        author: None,
        is_public: true,
        enabled: true,
        is_system: false,
        usage_count,
        created_at: template.created_at,
        updated_at: template.updated_at,
    }
}

pub(super) fn summary_from_trigger(
    template: wf_types::TriggerTemplateStorageMetadata,
    usage_count: u64,
) -> TemplateSummary {
    TemplateSummary {
        id: template.id.to_string(),
        kind: TemplateKind::Trigger.as_str(),
        name: template.name,
        description: template.description.unwrap_or_default(),
        category: template.category,
        tags: template.tags,
        author: None,
        is_public: true,
        enabled: template.enabled,
        is_system: false,
        usage_count,
        created_at: template.created_at,
        updated_at: template.updated_at,
    }
}
