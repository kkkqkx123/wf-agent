use wf_types::{FragmentCompositionConfig, SystemPromptFragment, TemplateVariableDefinition};

use crate::registry::{register_item_skip, register_item_strict, RegisterOptions, ResourceRegistries};
use crate::result::Summary;

pub fn builtin_fragments() -> Vec<SystemPromptFragment> {
    vec![
        SystemPromptFragment {
            id: "fragments.role.assistant".into(),
            category: "role".into(),
            content: "You are a helpful AI assistant.".into(),
            description: Some("Base assistant role definition".into()),
            variables: None,
        },
        SystemPromptFragment {
            id: "fragments.role.coder".into(),
            category: "role".into(),
            content: "You are an expert software engineer. You write clean, idiomatic, well-structured code.".into(),
            description: Some("Code-focused assistant role".into()),
            variables: None,
        },
        SystemPromptFragment {
            id: "fragments.role.analyst".into(),
            category: "role".into(),
            content: "You are a data analyst. You analyze data, identify patterns, and present clear insights.".into(),
            description: Some("Data analyst role".into()),
            variables: None,
        },
        SystemPromptFragment {
            id: "fragments.capability.general".into(),
            category: "capability".into(),
            content: "You have access to a set of tools that you can use to accomplish tasks. Use them when appropriate.".into(),
            description: Some("General tool usage capability".into()),
            variables: None,
        },
        SystemPromptFragment {
            id: "fragments.capability.general-principles".into(),
            category: "capability".into(),
            content: "You should think step by step, be precise, and verify your work before declaring completion.".into(),
            description: Some("General AI principles".into()),
            variables: None,
        },
        SystemPromptFragment {
            id: "fragments.capability.coding".into(),
            category: "capability".into(),
            content: "Your knowledge cutoff is {cutoff_date}. For information after that date, use web search tools if available.".into(),
            description: Some("Coding capability with knowledge cutoff".into()),
            variables: Some(vec![TemplateVariableDefinition {
                name: "cutoff_date".into(),
                r#type: "string".into(),
                required: true,
                description: Some("Knowledge cutoff date".into()),
                default_value: None,
            }]),
        },
        SystemPromptFragment {
            id: "fragments.constraint.general".into(),
            category: "constraint".into(),
            content: "You must not provide harmful, illegal, or unethical advice. If a request violates these principles, politely decline.".into(),
            description: Some("General safety constraint".into()),
            variables: None,
        },
        SystemPromptFragment {
            id: "fragments.constraint.general-interaction".into(),
            category: "constraint".into(),
            content: "You must protect user privacy and confidential information. Never share or expose sensitive data.".into(),
            description: Some("Interaction constraint".into()),
            variables: None,
        },
        SystemPromptFragment {
            id: "fragments.constraint.coding".into(),
            category: "constraint".into(),
            content: "Write code that is correct, maintainable, and follows language-specific best practices. Always handle errors gracefully.".into(),
            description: Some("Coding quality constraint".into()),
            variables: None,
        },
        SystemPromptFragment {
            id: "fragments.constraint.code-safety".into(),
            category: "constraint".into(),
            content: "Never introduce security vulnerabilities. Validate inputs, avoid injection risks, and review code for unsafe patterns.".into(),
            description: Some("Code safety constraint".into()),
            variables: None,
        },
        SystemPromptFragment {
            id: "fragments.tool-usage.xml-summary".into(),
            category: "tool-usage".into(),
            content: "When using tools:\n1. Think step by step about which tool to use\n2. Provide the correct parameters\n3. Review the results before proceeding".into(),
            description: Some("XML-style tool usage guidelines".into()),
            variables: None,
        },
        SystemPromptFragment {
            id: "fragments.tool-usage.json-summary".into(),
            category: "tool-usage".into(),
            content: "When working with files:\n- Read files before editing them\n- Make focused, minimal changes\n- Verify your changes after writing".into(),
            description: Some("JSON-style tool usage guidelines".into()),
            variables: None,
        },
        SystemPromptFragment {
            id: "fragments.task-instruction.code-review".into(),
            category: "task-instruction".into(),
            content: "Work through the task step by step. If you need more information, ask the user.".into(),
            description: Some("Code review task instruction".into()),
            variables: None,
        },
        SystemPromptFragment {
            id: "fragments.task-instruction.data-analysis".into(),
            category: "task-instruction".into(),
            content: "Before starting, break down the task into steps. For each step, decide what tools you need and execute them. Review your progress after each step.".into(),
            description: Some("Data analysis task instruction".into()),
            variables: None,
        },
    ]
}

pub fn builtin_compositions() -> Vec<FragmentCompositionConfig> {
    builtin_composition_entries()
        .into_iter()
        .map(|(_, cfg)| cfg)
        .collect()
}

/// Labeled builtin compositions: the single data source the prompt builder
/// reads (the builder no longer hardcodes fragment ids per prompt type).
pub fn builtin_composition_entries() -> Vec<(&'static str, FragmentCompositionConfig)> {
    vec![
        (
            "assistant",
            FragmentCompositionConfig {
                fragment_ids: vec![
                    "fragments.role.assistant".into(),
                    "fragments.capability.general".into(),
                    "fragments.capability.general-principles".into(),
                    "fragments.constraint.general".into(),
                    "fragments.tool-usage.xml-summary".into(),
                    "fragments.task-instruction.code-review".into(),
                ],
                separator: Some("\n\n".into()),
                prefix: None,
                suffix: None,
            },
        ),
        (
            "coder",
            FragmentCompositionConfig {
                fragment_ids: vec![
                    "fragments.role.coder".into(),
                    "fragments.capability.general".into(),
                    "fragments.capability.coding".into(),
                    "fragments.constraint.coding".into(),
                    "fragments.constraint.code-safety".into(),
                    "fragments.tool-usage.json-summary".into(),
                    "fragments.task-instruction.code-review".into(),
                ],
                separator: Some("\n\n".into()),
                prefix: None,
                suffix: None,
            },
        ),
    ]
}

/// Look up the builtin composition for a prompt kind (`"assistant"` /
/// `"coder"`). Unknown kinds return `None`.
pub fn builtin_composition_for(kind: &str) -> Option<FragmentCompositionConfig> {
    builtin_composition_entries()
        .into_iter()
        .find(|(label, _)| *label == kind)
        .map(|(_, cfg)| cfg)
}

pub fn register(regs: &ResourceRegistries, opts: &RegisterOptions) -> Summary {
    let mut total = Summary::new();
    for fragment in builtin_fragments() {
        let id = fragment.id.clone();
        total.merge(if opts.skip_if_exists {
            register_item_skip(&regs.fragments, id, fragment)
        } else {
            register_item_strict(&regs.fragments, id, fragment)
        });
    }
    total
}
