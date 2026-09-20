use wf_core::registry::Registry;

use crate::custom::types::{CustomPromptDefinition, CustomPromptType};
use crate::registry::{register_item_skip, register_item_strict, ResourceRegistries};
use crate::result::Summary;
use wf_types::Template;

pub fn register_custom_prompts(
    regs: &ResourceRegistries,
    prompts: Vec<CustomPromptDefinition>,
    skip_if_exists: bool,
) -> Summary {
    let mut total = Summary::new();

    for p in prompts {
        let category = match p.prompt_type {
            CustomPromptType::System => "system",
            CustomPromptType::User => "user",
            CustomPromptType::Assistant => "assistant",
            CustomPromptType::Fragments => "fragments",
        };

        let variables = p.variables.map(|vars| {
            vars.into_iter()
                .map(|v| wf_types::TemplateVariableDefinition {
                    name: v.name,
                    r#type: v.var_type,
                    required: v.required.unwrap_or(false),
                    description: v.description,
                    default_value: v.default_value,
                })
                .collect()
        });

        // Declared fragments must exist so composition cannot silently
        // drop sections at render time.
        if let Some(ref fragment_ids) = p.fragments {
            let missing: Vec<&str> = fragment_ids
                .iter()
                .filter(|id| !regs.fragments.has(id))
                .map(String::as_str)
                .collect();
            if !missing.is_empty() {
                total.merge(Summary::err(
                    &p.id,
                    format!("references unregistered fragments: {}", missing.join(", ")),
                ));
                continue;
            }
        }

        let template = Template {
            id: p.id.clone(),
            name: p.name.clone(),
            description: Some(p.name),
            category: category.into(),
            content: p.content,
            variables,
            fragments: p.fragments,
        };

        if let Err(e) = wf_config::processor::prompt::validate_prompt_template(&template) {
            total.merge(Summary::err(&p.id, e.to_string()));
            continue;
        }

        total.merge(if skip_if_exists {
            register_item_skip(&regs.templates, p.id, template)
        } else {
            register_item_strict(&regs.templates, p.id, template)
        });
    }
    total
}
