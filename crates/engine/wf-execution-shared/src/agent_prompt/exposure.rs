use std::collections::{HashMap, HashSet};

use serde_json::Value;
use wf_resource::registry::ResourceRegistries;

use super::variables::PromptEnvironment;

/// Pre-rendered tool exposure blocks handed to the loop config.
pub struct ExposureArtifacts {
    pub general_description: Option<String>,
    pub discoverable_metadata_block: Option<String>,
}

/// Resolve exposure once and render the `general` description plus the
/// discoverable metadata block from that single resolution. Assembly-time
/// overrides and activations stay empty; per-turn resolution owns those.
/// `None` available means unset (inherit-all); `Some` empty with empty
/// initial and discoverable means an explicit pure chat run with no tools.
pub fn build_exposure_artifacts(
    env: &PromptEnvironment,
    tool_call_protocol: Option<&wf_types::llm::ToolCallProtocolConfig>,
    available_tool_names: Option<&[String]>,
    initial_tool_names: &[String],
    discoverable_tool_names: &[String],
    hidden_tool_names: &[String],
    enable_general_tool: Option<bool>,
) -> ExposureArtifacts {
    // Explicit pure chat run renders no blocks instead of resolving an
    // empty pool to all registered tools.
    if wf_tools::tool_exposure::is_explicit_no_tools_option(
        available_tool_names,
        initial_tool_names,
        discoverable_tool_names,
    ) {
        return ExposureArtifacts {
            general_description: None,
            discoverable_metadata_block: None,
        };
    }
    let metadata_options = wf_tools::discoverable_metadata_options(tool_call_protocol);
    let exposure_resolution = env.tool_registry.map(|registry| {
        wf_tools::resolve_tool_exposure(wf_tools::ExposureInput {
            registry,
            available: wf_tools::AvailablePool::from_option(available_tool_names),
            initial_names: initial_tool_names,
            discoverable_names: discoverable_tool_names,
            hidden_names: hidden_tool_names,
            enable_general_tool,
            activated_tools: &HashSet::new(),
            exposure_overrides: &HashMap::new(),
        })
    });

    let template_metrics = env.template_metrics();
    let general_description = render_general_description(
        exposure_resolution
            .as_ref()
            .map(|resolution| resolution.general_enabled)
            .unwrap_or(false),
        env.resource_registries,
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
        let variables =
            HashMap::from([("tool_list".to_string(), Value::String(entries.join("\n")))]);
        let block = env
            .resource_registries
            .and_then(|regs| {
                wf_resource::render_template_for_model(
                    regs,
                    wf_resource::DISCOVERABLE_METADATA_TEMPLATE_ID,
                    &variables,
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

fn render_general_description(
    general_enabled: bool,
    regs: Option<&ResourceRegistries>,
    tool_call_protocol: Option<&wf_types::llm::ToolCallProtocolConfig>,
    template_metrics: Option<&wf_metrics::TemplateMetricsCollector>,
) -> Option<String> {
    if !general_enabled {
        return None;
    }
    let regs = regs?;
    let format = tool_call_protocol.map(|f| &f.format);
    let variables = HashMap::from([
        (
            "tool_call_protocol".to_string(),
            Value::String(
                format
                    .map(|f| f.to_string())
                    .unwrap_or_else(|| "xml".to_string()),
            ),
        ),
        (
            "invoke_example".to_string(),
            Value::String(general_invoke_example(format)),
        ),
    ]);
    wf_resource::render_template_for_model(
        regs,
        wf_resource::GENERAL_DESCRIPTION_TEMPLATE_ID,
        &variables,
        template_metrics,
    )
}

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
