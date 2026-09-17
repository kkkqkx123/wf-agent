//! Tool-exposure assembly artifacts rendered at loop-assembly time: the single
//! exposure resolution shared by the `general` tool description and the
//! discoverable-tool metadata block, so metadata and schema never drift.

use std::collections::{HashMap, HashSet};

use wf_execution_shared::context::NodeExecutionContext;

/// The pre-rendered, templateable blocks handed to the loop config.
pub(crate) struct ExposureArtifacts {
    pub general_description: Option<String>,
    pub discoverable_metadata_block: Option<String>,
}

/// Resolve exposure once (config lists + metadata exposure, filtered by the
/// available pool and hidden blocklist) and render the `general` description
/// and the discoverable metadata block from that single resolution. Assembly-
/// time overrides and activations are empty (per-turn resolution owns those).
pub(crate) fn build_exposure_artifacts(
    ctx: &NodeExecutionContext,
    tool_call_protocol: Option<&wf_types::llm::ToolCallProtocolConfig>,
    available_tool_names: &[String],
    initial_tool_names: &[String],
    discoverable_tool_names: &[String],
    hidden_tool_names: &[String],
    enable_general_tool: Option<bool>,
) -> ExposureArtifacts {
    let metadata_options = wf_tools::discoverable_metadata_options(tool_call_protocol);
    let exposure_resolution = ctx.tool_registry.as_ref().map(|registry| {
        wf_tools::resolve_tool_exposure(wf_tools::ExposureInput {
            registry: registry.as_ref(),
            available_names: available_tool_names,
            initial_names: initial_tool_names,
            discoverable_names: discoverable_tool_names,
            hidden_names: hidden_tool_names,
            enable_general_tool,
            activated_tools: &HashSet::new(),
            exposure_overrides: &HashMap::new(),
        })
    });

    let template_metrics = ctx.metrics.as_ref().map(|m| m.template());
    let general_description = render_general_description(
        exposure_resolution
            .as_ref()
            .map(|resolution| resolution.general_enabled)
            .unwrap_or(false),
        ctx.resource_registries.as_deref(),
        tool_call_protocol,
        template_metrics.as_deref(),
    );

    let discoverable_metadata_block = exposure_resolution.as_ref().and_then(|resolution| {
        if !resolution.general_enabled || resolution.discoverable.is_empty() {
            return None;
        }
        let entries = wf_tools::generate_discoverable_tool_entries_with_options(
            &resolution.discoverable,
            &metadata_options,
        );
        let variables = HashMap::from([("tool_list".to_string(), entries.join("\n"))]);
        let block = ctx
            .resource_registries
            .as_deref()
            .and_then(|regs| {
                wf_resource::render_template_with_metrics(
                    regs,
                    wf_resource::DISCOVERABLE_METADATA_TEMPLATE_ID,
                    &wf_resource::TemplateRenderOptions {
                        variables: variables.clone(),
                        ..Default::default()
                    },
                    template_metrics.as_deref(),
                )
            })
            .unwrap_or_else(|| {
                wf_tools::generate_discoverable_tools_metadata_with_options(
                    &resolution.discoverable,
                    &metadata_options,
                )
            });
        Some(block)
    });

    ExposureArtifacts {
        general_description,
        discoverable_metadata_block,
    }
}

/// Render the `general` tool description at loop assembly time.
///
/// The text comes from the `tool-visibility.general_description` resource
/// template (variables: `tool_call_protocol`, `invoke_example`), so it follows
/// custom resource overrides and the effective tool call format; the
/// per-turn schema assembly (wf-agent) writes the rendered text into the
/// routed tool copy. Executions without injected registries fall back to
/// `None` (the builtin static description, current behavior).
fn render_general_description(
    general_enabled: bool,
    regs: Option<&wf_resource::ResourceRegistries>,
    tool_call_protocol: Option<&wf_types::llm::ToolCallProtocolConfig>,
    template_metrics: Option<&wf_metrics::TemplateMetricsCollector>,
) -> Option<String> {
    if !general_enabled {
        return None;
    }
    let regs = regs?;
    let format = tool_call_protocol.map(|f| &f.format);
    let mut variables = HashMap::new();
    variables.insert(
        "tool_call_protocol".to_string(),
        format
            .map(|f| f.to_string())
            .unwrap_or_else(|| "xml".to_string()),
    );
    variables.insert("invoke_example".to_string(), general_invoke_example(format));
    wf_resource::render_template_with_metrics(
        regs,
        wf_resource::GENERAL_DESCRIPTION_TEMPLATE_ID,
        &wf_resource::TemplateRenderOptions {
            variables,
            ..Default::default()
        },
        template_metrics,
    )
}

/// Invoke example for the `general` tool description, adapted to the outer
/// tool call format: XML formats teach the `<tool_use>`-wrapped JSON body;
/// JSON formats teach the bare JSON body the model must emit.
fn general_invoke_example(format: Option<&wf_types::llm::ToolCallProtocol>) -> String {
    match format {
        Some(wf_types::llm::ToolCallProtocol::JsonWrapped)
        | Some(wf_types::llm::ToolCallProtocol::JsonRaw)
        | Some(wf_types::llm::ToolCallProtocol::Native) => {
            "{\"tool\": \"general\", \"parameters\": {\"request\": \"{\\\"tool\\\": \\\"web_search\\\", \
             \\\"parameters\\\": {\\\"query\\\": \\\"rust\\\"}}\"}}"
                .to_string()
        }
        _ => "<tool_use>\n  <tool_name>general</tool_name>\n  <parameters>\n    \
              <request>{\"tool\": \"web_search\", \"parameters\": {\"query\": \"rust\"}}</request>\n  \
              </parameters>\n</tool_use>"
            .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn general_description_follows_template_resource() {
        // No registries: falls back to None (builtin static description).
        assert!(render_general_description(true, None, None, None).is_none());
        assert!(render_general_description(false, None, None, None).is_none());

        // A custom template override wins and receives the format variable.
        let regs = wf_resource::ResourceRegistries::new();
        use wf_core::registry::MutableRegistry;
        regs.templates
            .register(
                wf_resource::GENERAL_DESCRIPTION_TEMPLATE_ID.to_string(),
                std::sync::Arc::new(wf_types::Template {
                    id: wf_resource::GENERAL_DESCRIPTION_TEMPLATE_ID.into(),
                    name: "custom general".into(),
                    description: None,
                    category: "tool-visibility".into(),
                    content: "Call inner tools with format={tool_call_protocol}".into(),
                    variables: None,
                    fragments: None,
                }),
            )
            .unwrap();

        let format = Some(wf_types::llm::ToolCallProtocolConfig {
            format: wf_types::llm::ToolCallProtocol::Xml,
            markers: None,
            xml_tags: None,
            include_description: None,
            description_style: None,
            include_examples: None,
            include_rules: None,
            additional_config: None,
        });
        let rendered = render_general_description(true, Some(&regs), format.as_ref(), None)
            .expect("configured template must render");
        assert!(rendered.contains("format=xml"), "got: {}", rendered);

        // Disabled general: nothing is rendered even with registries.
        assert!(render_general_description(false, Some(&regs), format.as_ref(), None).is_none());

        // Unregistered id fallback: the builtin default still renders.
        regs.templates
            .unregister(wf_resource::GENERAL_DESCRIPTION_TEMPLATE_ID);
        let builtin = render_general_description(true, Some(&regs), format.as_ref(), None)
            .expect("builtin default must render");
        assert!(builtin.contains("<tool_use>"));
        assert!(builtin.contains("web_search"));

        // JsonWrapped: the builtin example switches to the bare JSON body.
        let json_format = Some(wf_types::llm::ToolCallProtocolConfig {
            format: wf_types::llm::ToolCallProtocol::JsonWrapped,
            markers: None,
            xml_tags: None,
            include_description: None,
            description_style: None,
            include_examples: None,
            include_rules: None,
            additional_config: None,
        });
        let json_builtin =
            render_general_description(true, Some(&regs), json_format.as_ref(), None)
                .expect("builtin default must render");
        assert!(json_builtin.contains("\"tool\": \"general\""));
        assert!(
            !json_builtin.contains("<tool_use>"),
            "JSON formats must not teach the XML wrapper: {}",
            json_builtin
        );
    }
}
