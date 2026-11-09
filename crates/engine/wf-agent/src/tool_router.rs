//! Per-turn tool assembly (single convergence point).
//!
//! [`ToolRouter`] carries the visible tool set for one agent iteration
//! assembled from the whitelist, the exposure layers and the tool discovery
//! state. The computation itself lives in
//! [`wf_tools::resolve_tool_exposure`] so the agent-loop metadata injection
//! (wf-workflow `AGENT_LOOP` assembly) consumes the same inputs, filters and
//! decisions — schema and metadata can never drift apart.
//!
//! Assembly is KV-cache friendly by design: the visible schema
//! is derived only from whitelist + exposure layers + formal activation.
//! Runtime `block` actions never change the schema — they only intercept at
//! execution time — so the LLM request prefix stays byte-stable across
//! turns.
//!
//! Exposure layers (strongest first):
//! 1. hidden blocklist → `Hidden`
//! 2. per-run exposure overrides (guardian / reviewer forms)
//! 3. discoverable config list → `Discoverable` (metadata-only)
//! 4. tool metadata exposure (default `Direct`)
//!
//! The `general` tool is appended to the visible set when the discoverable
//! list is non-empty and not explicitly disabled.

use wf_tools::tool_exposure::ExposureResolution;

/// The assembled tool set for one turn.
#[derive(Debug, Clone, Default)]
pub struct ToolRouter {
    /// Tools visible to the model this turn: initial set + formally
    /// activated gated tools + (conditionally) the `general` tool. Rendered
    /// into the LLM request.
    pub visible: Vec<wf_types::tool::Tool>,
    /// Discoverable tools: only metadata is injected into the prompt; calls
    /// go through the `general` tool until formally activated.
    pub discoverable: Vec<wf_types::tool::Tool>,
    /// Gated tools (available but not initial): not visible until activated
    /// via TOOL_VISIBILITY unblock.
    pub gated: Vec<wf_types::tool::Tool>,
    /// Hidden tools: registered but never exposed to the model.
    pub hidden: Vec<wf_types::tool::Tool>,
}

impl From<ExposureResolution> for ToolRouter {
    fn from(resolution: ExposureResolution) -> Self {
        Self {
            visible: resolution.visible,
            discoverable: resolution.discoverable,
            gated: resolution.gated,
            hidden: resolution.hidden,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};
    use wf_types::tool::{Tool, ToolExposure, ToolMetadata, ToolType};

    fn make_tool(id: &str, name: &str, exposure: Option<ToolExposure>) -> Tool {
        Tool {
            id: id.into(),
            name: name.into(),
            description: format!("tool {}", name),
            tool_type: ToolType::Stateless,
            parameters: None,
            metadata: exposure.map(|exposure| ToolMetadata {
                category: None,
                tags: None,
                documentation_url: None,
                custom_fields: None,
                risk_level: None,
                auto_approvable: None,
                create_checkpoint: None,
                exposure: Some(exposure),
            }),
            config: None,
            enabled: Some(true),
            strict: None,
            default_timeout_ms: None,
        }
    }

    fn general_tool() -> Tool {
        let mut tool = wf_tools::predefined::general::GENERAL.tool_def();
        tool.id = "general".into();
        tool
    }

    fn registry_with(tools: Vec<Tool>) -> wf_tools::registry::ToolRegistry {
        let registry = wf_tools::registry::ToolRegistry::new();
        for tool in tools {
            registry.register_tool(tool);
        }
        registry
    }

    #[test]
    fn test_router_converts_shared_resolution() {
        let registry = registry_with(vec![
            make_tool("a", "alpha", None),
            make_tool("d", "web_search", None),
            general_tool(),
        ]);
        let resolution = wf_tools::resolve_tool_exposure(wf_tools::ExposureInput {
            registry: &registry,
            available: wf_tools::AvailablePool::Explicit(&[
                "alpha".to_string(),
                "web_search".to_string(),
                "general".to_string(),
            ]),
            initial_names: &["alpha".to_string()],
            discoverable_names: &["web_search".to_string()],
            hidden_names: &[],
            enable_general_tool: None,
            activated_tools: &HashSet::new(),
            exposure_overrides: &HashMap::new(),
        });
        let router = ToolRouter::from(resolution);
        let visible: Vec<String> = router.visible.iter().map(|t| t.name.clone()).collect();
        assert_eq!(visible, vec!["alpha".to_string(), "general".to_string()]);
        let discoverable: Vec<String> =
            router.discoverable.iter().map(|t| t.name.clone()).collect();
        assert_eq!(discoverable, vec!["web_search".to_string()]);
    }

    #[test]
    fn test_rag_search_participates_in_single_turn_assembly() {
        let registry = registry_with(vec![
            wf_tools::predefined::rag::RAG_SEARCH.tool_def(),
            general_tool(),
        ]);
        let resolution = wf_tools::resolve_tool_exposure(wf_tools::ExposureInput {
            registry: &registry,
            available: wf_tools::AvailablePool::Explicit(&["rag_search".to_string()]),
            initial_names: &["rag_search".to_string()],
            discoverable_names: &[],
            hidden_names: &[],
            enable_general_tool: Some(false),
            activated_tools: &HashSet::new(),
            exposure_overrides: &HashMap::new(),
        });
        let router = ToolRouter::from(resolution);
        assert!(router.visible.iter().any(|t| t.name == "rag_search"));
    }
}
