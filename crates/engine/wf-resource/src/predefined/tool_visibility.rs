use serde::Deserialize;
use wf_types::Template;

use crate::registry::{register_template, RegisterOptions, ResourceRegistries};
use crate::result::Summary;

#[derive(Debug, Deserialize)]
struct ToolVisibilityFile {
    templates: Vec<Template>,
}

fn embedded_templates() -> ToolVisibilityFile {
    serde_json::from_str(include_str!("../../configs/tool_visibility.json"))
        .expect("embedded tool_visibility.json is valid")
}

pub fn builtin_tool_visibility_templates() -> Vec<Template> {
    embedded_templates().templates
}

pub const ACTIVATION_TEMPLATE_ID: &str = "tool-visibility.activation";
pub const BLOCK_TEMPLATE_ID: &str = "tool-visibility.block";
pub const DISCOVERABLE_METADATA_TEMPLATE_ID: &str = "tool-visibility.discoverable_metadata";
pub const GENERAL_DESCRIPTION_TEMPLATE_ID: &str = "tool-visibility.general_description";

pub const ACTIVATION_CONTENT: &str = "[Tool Activation] The following tools are now available: {{tool_names}}.\nYou can call them directly or via the general tool.";
pub const BLOCK_CONTENT: &str = "The following tools are now unavailable:\n{{tool_names}}";
pub const DISCOVERABLE_METADATA_CONTENT: &str =
    "Discoverable tools:\n{{tool_list}}\nInvoke them via the general tool.";
pub const GENERAL_DESCRIPTION_CONTENT: &str = "Invoke tools whose schemas are not directly exposed. The request body is a JSON object {\"tool\": \"tool_name\", \"parameters\": {...}} passed as the `request` parameter, e.g.:\n{{invoke_example}}\nThe inner tool is interpreted and executed server-side.";

pub const GENERIC_VISIBILITY_CONTENT: &str =
    "Tool visibility changed ({{action}}):\n{{tool_names}}";

pub fn generic_visibility_text(action: &str, tool_list: &str) -> String {
    wf_common::template::apply_template_variables(
        GENERIC_VISIBILITY_CONTENT,
        &std::collections::HashMap::from([
            ("action".to_string(), action.to_string()),
            ("tool_names".to_string(), tool_list.to_string()),
        ]),
    )
}

pub fn register(regs: &ResourceRegistries, opts: &RegisterOptions) -> Summary {
    let mut total = Summary::new();
    for template in builtin_tool_visibility_templates() {
        total.merge(register_template(regs, template, opts.skip_if_exists));
    }
    total
}
