use wf_types::Template;

use crate::registry::{register_item_skip, register_item_strict, RegisterOptions, ResourceRegistries};
use crate::result::Summary;

pub fn builtin_prompts() -> Vec<Template> {
    vec![
        Template {
            id: "system.default".into(),
            name: "Default System Prompt".into(),
            description: Some("Default system prompt for general-purpose assistant".into()),
            category: "system".into(),
            content: "{fragments}\n\n{tool_descriptions}".into(),
            variables: None,
            fragments: Some(vec![
                "fragments.role.assistant".into(),
                "fragments.capability.general".into(),
                "fragments.constraint.general".into(),
                "fragments.tool-usage.xml-summary".into(),
                "fragments.task-instruction.code-review".into(),
            ]),
        },
        Template {
            id: "system.code".into(),
            name: "Code Assistant System Prompt".into(),
            description: Some("System prompt specialized for code generation and analysis".into()),
            category: "system".into(),
            content: "{fragments}\n\n{tool_descriptions}".into(),
            variables: None,
            fragments: Some(vec![
                "fragments.role.coder".into(),
                "fragments.capability.general".into(),
                "fragments.capability.coding".into(),
                "fragments.constraint.coding".into(),
                "fragments.constraint.code-safety".into(),
                "fragments.tool-usage.json-summary".into(),
                "fragments.task-instruction.code-review".into(),
            ]),
        },
        Template {
            id: "system.agent".into(),
            name: "Agent System Prompt".into(),
            description: Some("System prompt for autonomous agent mode".into()),
            category: "system".into(),
            content: "{fragments}\n\n{tool_descriptions}".into(),
            variables: None,
            fragments: Some(vec![
                "fragments.role.assistant".into(),
                "fragments.capability.general".into(),
                "fragments.capability.coding".into(),
                "fragments.constraint.general".into(),
                "fragments.tool-usage.xml-summary".into(),
                "fragments.task-instruction.data-analysis".into(),
            ]),
        },
    ]
}

pub fn register(regs: &ResourceRegistries, opts: &RegisterOptions) -> Summary {
    let mut total = Summary::new();
    for prompt in builtin_prompts() {
        let id = prompt.id.clone();
        total.merge(if opts.skip_if_exists {
            register_item_skip(&regs.templates, id, prompt)
        } else {
            register_item_strict(&regs.templates, id, prompt)
        });
    }
    total
}
