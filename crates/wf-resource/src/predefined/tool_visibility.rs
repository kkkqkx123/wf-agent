//! Predefined tool-visibility prompt templates.
//!
//! These templates drive the TOOL_VISIBILITY announcements, the
//! `{DISCOVERABLE_TOOLS_METADATA}` block and the `general` tool description.
//! They are registered into the unified template registry (so they can be
//! replaced by custom resources and hot-reloaded); the render engine falls
//! back to built-in defaults when nothing is configured.

use wf_types::Template;

use crate::registry::{register_item_skip, register_item_strict, RegisterOptions, ResourceRegistries};
use crate::result::Summary;

/// Ids of the four built-in tool visibility templates.
pub const ACTIVATION_TEMPLATE_ID: &str = "tool-visibility.activation";
pub const BLOCK_TEMPLATE_ID: &str = "tool-visibility.block";
pub const DISCOVERABLE_METADATA_TEMPLATE_ID: &str = "tool-visibility.discoverable_metadata";
pub const GENERAL_DESCRIPTION_TEMPLATE_ID: &str = "tool-visibility.general_description";

/// Built-in template texts (mirror of the previous hardcoded strings).
pub fn builtin_tool_visibility_templates() -> Vec<Template> {
    vec![
        Template {
            id: ACTIVATION_TEMPLATE_ID.into(),
            name: "Tool Activation Announcement".into(),
            description: Some("Tail system announcement after TOOL_VISIBILITY unblock".into()),
            category: "tool-visibility".into(),
            content: "[Tool Activation] The following tools are now available: {tool_names}.\n\
                      You can call them directly or via the general tool."
                .into(),
            variables: None,
            fragments: None,
        },
        Template {
            id: BLOCK_TEMPLATE_ID.into(),
            name: "Tool Block Announcement".into(),
            description: Some("Tail system announcement after TOOL_VISIBILITY block".into()),
            category: "tool-visibility".into(),
            content: "The following tools are now unavailable:\n{tool_names}".into(),
            variables: None,
            fragments: None,
        },
        Template {
            id: DISCOVERABLE_METADATA_TEMPLATE_ID.into(),
            name: "Discoverable Tools Metadata".into(),
            description: Some(
                "Discoverable tool metadata block injected into the system prompt".into(),
            ),
            category: "tool-visibility".into(),
            content: "Discoverable tools:\n{tool_list}\nInvoke them via the general tool.".into(),
            variables: None,
            fragments: None,
        },
        Template {
            id: GENERAL_DESCRIPTION_TEMPLATE_ID.into(),
            name: "General Tool Description".into(),
            description: Some("Description of the general tool shown to the model".into()),
            category: "tool-visibility".into(),
            content: "Invoke tools whose schemas are not directly exposed. The request body is a \
                      JSON object {\"tool\": \"tool_name\", \"parameters\": {...}} passed as the \
                      `request` parameter, e.g.:\n\
                      {invoke_example}\n\
                      The inner tool is interpreted and executed server-side."
                .into(),
            variables: None,
            fragments: None,
        },
    ]
}

pub fn register(regs: &ResourceRegistries, opts: &RegisterOptions) -> Summary {
    let mut total = Summary::new();
    for template in builtin_tool_visibility_templates() {
        let id = template.id.clone();
        total.merge(if opts.skip_if_exists {
            register_item_skip(&regs.templates, id, template)
        } else {
            register_item_strict(&regs.templates, id, template)
        });
    }
    total
}
