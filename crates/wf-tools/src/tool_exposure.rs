//! Single-source per-turn tool exposure resolution.
//!
//! [`resolve_tool_exposure`] computes the tool buckets (visible /
//! discoverable / gated / hidden) and the `general` tool exposure decision
//! from one shared set of inputs. Both the per-turn schema assembly
//! (wf-agent `build_tool_router`) and the agent-loop discoverable metadata
//! injection (wf-workflow `AGENT_LOOP` handler assembly) consume this
//! function, so the two can never drift apart: the injected metadata always
//! describes exactly the tools the assembled schema exposes, and vice versa.
//!
//! The function is pure: it only reads the registered tool list and applies
//! the filtering rules below, with no registration, engine state or side
//! effects.
//!
//! Exposure layers (strongest first):
//! 1. hidden blocklist → `Hidden`
//! 2. per-run exposure overrides (guardian / reviewer forms)
//! 3. discoverable config list → `Discoverable` (metadata-only)
//! 4. tool metadata exposure (default `Direct`)
//!
//! The `general` tool is appended to the visible set when the discoverable
//! list is non-empty and not explicitly disabled; it is exempt from the
//! whitelist (assembly-layer injection, like `skill`).

use std::collections::{HashMap, HashSet};

use wf_types::tool::{Tool, ToolExposure};

use crate::general::GENERAL_TOOL_NAME;
use crate::registry::ToolRegistry;

/// Inputs for one exposure resolution.
pub struct ExposureInput<'a> {
    pub registry: &'a ToolRegistry,
    /// Whitelist of tool names (the available pool); empty means all
    /// registered tools.
    pub available_names: &'a [String],
    /// Tools visible in the initial schema. When empty, all available tools
    /// are initially visible.
    pub initial_names: &'a [String],
    /// Discoverable tools (config-level; supplements metadata).
    pub discoverable_names: &'a [String],
    /// Explicit hidden / blocklist names (config-level; supplements
    /// metadata).
    pub hidden_names: &'a [String],
    /// Escape hatch controlling `general` tool exposure; `None` means auto
    /// (exposed iff the discoverable list is non-empty).
    pub enable_general_tool: Option<bool>,
    /// Tools formally activated via TOOL_VISIBILITY unblock; gated tools in
    /// this set enter the visible schema.
    pub activated_tools: &'a HashSet<String>,
    /// Per-tool exposure overrides; stronger than config lists but weaker
    /// than the hidden blocklist.
    pub exposure_overrides: &'a HashMap<String, ToolExposure>,
}

/// The resolved tool buckets for one assembly plus the `general` tool
/// decision.
#[derive(Debug, Clone, Default)]
pub struct ExposureResolution {
    /// Tools visible to the model this turn: initial set + formally
    /// activated gated tools + (conditionally) the `general` tool. Rendered
    /// into the LLM request.
    pub visible: Vec<Tool>,
    /// Discoverable tools: only metadata is injected into the prompt; calls
    /// go through the `general` tool until formally activated.
    pub discoverable: Vec<Tool>,
    /// Gated tools (available but not initial): not visible until activated
    /// via TOOL_VISIBILITY unblock.
    pub gated: Vec<Tool>,
    /// Hidden tools: registered but never exposed to the model.
    pub hidden: Vec<Tool>,
    /// Whether the `general` tool is actually exposed this turn: auto-enabled
    /// when the resolved discoverable list is non-empty, disabled explicitly,
    /// or disabled when the tool is not registered. The single decision
    /// source for both schema assembly and metadata injection (the two must
    /// stay in lockstep).
    pub general_enabled: bool,
}

/// Resolve the tool buckets and the `general` tool decision for one
/// assembly. `Discoverable` tools never enter the schema (metadata only),
/// `Hidden` tools never appear at all, gated tools appear only when formally
/// activated, and the whitelist is layered on top. Runtime visibility blocks
/// do NOT filter the schema.
///
/// Metadata consumers (discoverable tool listing) must use this same
/// resolution rather than re-deriving the discoverable set from config
/// lists: the returned `discoverable` bucket is already filtered by the
/// available pool and the hidden blocklist.
pub fn resolve_tool_exposure(input: ExposureInput<'_>) -> ExposureResolution {
    let mut visible = Vec::new();
    let mut discoverable = Vec::new();
    let mut gated = Vec::new();
    let mut hidden = Vec::new();
    let pool_empty = input.available_names.is_empty();
    let initial_empty = input.initial_names.is_empty();

    for tool in input.registry.list_tools() {
        if !pool_empty && !input.available_names.contains(&tool.name) {
            continue;
        }

        match effective_exposure(&tool, &input) {
            ToolExposure::Discoverable => discoverable.push(tool),
            ToolExposure::Hidden => hidden.push(tool),
            ToolExposure::Direct | ToolExposure::DirectModelOnly => {
                let explicitly_initial = input.initial_names.contains(&tool.name);
                let explicitly_overridden = input.exposure_overrides.contains_key(&tool.name);
                let gated_by_config =
                    !initial_empty && !explicitly_initial && !explicitly_overridden;
                if gated_by_config && !input.activated_tools.contains(&tool.name) {
                    gated.push(tool);
                } else {
                    visible.push(tool);
                }
            }
        }
    }

    // The `general` tool enters the initial visible set when discoverable
    // tools exist and it is not explicitly disabled; it is exempt from the
    // whitelist (assembly-layer injection, like `skill`). When the tool is
    // not registered, `general_enabled` falls back to false so schema and
    // metadata consumers share one decision.
    let mut general_enabled = input
        .enable_general_tool
        .unwrap_or(!discoverable.is_empty());
    if general_enabled {
        if let Some(general) = input
            .registry
            .list_tools()
            .into_iter()
            .find(|t| t.name == GENERAL_TOOL_NAME)
        {
            visible.push(general);
        } else {
            general_enabled = false;
        }
    }

    // Deterministic ordering: the registry iterates a concurrent map with a
    // random seed, so the raw listing order is unstable across processes.
    // Sorting by name makes the assembled schema (and thus the LLM request
    // prefix) byte-stable, which the KV-cache friendly design relies on.
    visible.sort_by(|a, b| a.name.cmp(&b.name));
    discoverable.sort_by(|a, b| a.name.cmp(&b.name));
    gated.sort_by(|a, b| a.name.cmp(&b.name));
    hidden.sort_by(|a, b| a.name.cmp(&b.name));

    ExposureResolution {
        visible,
        discoverable,
        gated,
        hidden,
        general_enabled,
    }
}

/// Resolve the effective exposure of a tool: hidden blocklist wins, then
/// runtime overrides, then the discoverable config list, then the tool's
/// declared metadata exposure (default `Direct`).
fn effective_exposure(tool: &Tool, input: &ExposureInput<'_>) -> ToolExposure {
    if input.hidden_names.contains(&tool.name) {
        return ToolExposure::Hidden;
    }
    if let Some(exposure) = input.exposure_overrides.get(&tool.name) {
        return *exposure;
    }
    if input.discoverable_names.contains(&tool.name) {
        return ToolExposure::Discoverable;
    }
    tool.metadata
        .as_ref()
        .and_then(|m| m.exposure)
        .unwrap_or_default()
}

/// Whether a tool is callable in this resolution: `visible` tools (direct
/// or model-only) and `discoverable` tools are callable (directly or via
/// the `general` tool); `gated` (not yet activated) and `hidden` tools are
/// not. Single-source runtime callability check: it consumes the same
/// resolution the schema assembly uses, so runtime gating and the assembled
/// schema can never drift apart.
pub fn is_tool_callable(resolution: &ExposureResolution, tool_name: &str) -> bool {
    resolution.visible.iter().any(|t| t.name == tool_name)
        || resolution.discoverable.iter().any(|t| t.name == tool_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::tool::{ToolMetadata, ToolType};

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
        let mut tool = crate::predefined::general::GENERAL.tool_def();
        tool.id = "general".into();
        tool
    }

    fn registry_with(tools: Vec<Tool>) -> ToolRegistry {
        let registry = ToolRegistry::new();
        for tool in tools {
            registry.register_tool(tool);
        }
        registry
    }

    fn input<'a>(
        registry: &'a ToolRegistry,
        available: &'a [String],
        initial: &'a [String],
        discoverable: &'a [String],
        hidden: &'a [String],
        overrides: &'a HashMap<String, ToolExposure>,
        activated: &'a HashSet<String>,
    ) -> ExposureInput<'a> {
        ExposureInput {
            registry,
            available_names: available,
            initial_names: initial,
            discoverable_names: discoverable,
            hidden_names: hidden,
            enable_general_tool: None,
            activated_tools: activated,
            exposure_overrides: overrides,
        }
    }

    fn names(resolution: &ExposureResolution) -> Vec<String> {
        resolution.visible.iter().map(|t| t.name.clone()).collect()
    }

    #[test]
    fn test_default_exposure_is_direct() {
        let registry = registry_with(vec![make_tool("a", "alpha", None)]);
        let resolution = resolve_tool_exposure(input(
            &registry,
            &[],
            &[],
            &[],
            &[],
            &Default::default(),
            &Default::default(),
        ));
        assert_eq!(names(&resolution), vec!["alpha".to_string()]);
        assert!(resolution.discoverable.is_empty());
        assert!(resolution.gated.is_empty());
        assert!(resolution.hidden.is_empty());
        assert!(!resolution.general_enabled);
    }

    #[test]
    fn test_initial_empty_keeps_all_available_visible() {
        let registry = registry_with(vec![
            make_tool("a", "alpha", None),
            make_tool("b", "beta", None),
        ]);
        let resolution = resolve_tool_exposure(input(
            &registry,
            &["alpha".to_string(), "beta".to_string()],
            &[],
            &[],
            &[],
            &Default::default(),
            &Default::default(),
        ));
        assert_eq!(
            names(&resolution),
            vec!["alpha".to_string(), "beta".to_string()]
        );
    }

    #[test]
    fn test_gated_tools_not_visible_until_activated() {
        let registry = registry_with(vec![
            make_tool("a", "alpha", None),
            make_tool("b", "beta", None),
            make_tool("c", "gamma", None),
        ]);
        // initial = [alpha]; beta/gamma are gated.
        let resolution = resolve_tool_exposure(input(
            &registry,
            &["alpha".to_string(), "beta".to_string(), "gamma".to_string()],
            &["alpha".to_string()],
            &[],
            &[],
            &Default::default(),
            &Default::default(),
        ));
        assert_eq!(names(&resolution), vec!["alpha".to_string()]);
        assert_eq!(resolution.gated.len(), 2);

        // Activating beta promotes it into the schema.
        let activated: HashSet<String> = ["beta".to_string()].into_iter().collect();
        let resolution = resolve_tool_exposure(input(
            &registry,
            &["alpha".to_string(), "beta".to_string(), "gamma".to_string()],
            &["alpha".to_string()],
            &[],
            &[],
            &Default::default(),
            &activated,
        ));
        let mut visible = names(&resolution);
        visible.sort();
        assert_eq!(visible, vec!["alpha".to_string(), "beta".to_string()]);
        assert_eq!(resolution.gated.len(), 1);
    }

    #[test]
    fn test_discoverable_not_in_schema_and_general_injected() {
        let registry = registry_with(vec![
            make_tool("a", "alpha", None),
            make_tool("d", "web_search", None),
            general_tool(),
        ]);
        let resolution = resolve_tool_exposure(input(
            &registry,
            &["alpha".to_string(), "web_search".to_string()],
            &["alpha".to_string()],
            &["web_search".to_string()],
            &[],
            &Default::default(),
            &Default::default(),
        ));
        // web_search is NOT in the schema; general IS injected.
        assert_eq!(
            names(&resolution),
            vec!["alpha".to_string(), "general".to_string()]
        );
        let discoverable: Vec<String> = resolution
            .discoverable
            .iter()
            .map(|t| t.name.clone())
            .collect();
        assert_eq!(discoverable, vec!["web_search".to_string()]);
        assert!(resolution.general_enabled);
    }

    #[test]
    fn test_general_disabled_when_explicitly_turned_off() {
        let registry = registry_with(vec![make_tool("d", "web_search", None), general_tool()]);
        let available = vec!["web_search".to_string()];
        let discoverable = vec!["web_search".to_string()];
        let empty: Vec<String> = Vec::new();
        let overrides = HashMap::new();
        let activated = HashSet::new();
        let mut input = input(
            &registry,
            &available,
            &empty,
            &discoverable,
            &empty,
            &overrides,
            &activated,
        );
        input.enable_general_tool = Some(false);
        let resolution = resolve_tool_exposure(input);
        assert!(names(&resolution).is_empty());
        assert!(!resolution.general_enabled);
    }

    #[test]
    fn test_general_absent_from_registry_is_skipped() {
        let registry = registry_with(vec![make_tool("d", "web_search", None)]);
        let resolution = resolve_tool_exposure(input(
            &registry,
            &["web_search".to_string()],
            &[],
            &["web_search".to_string()],
            &[],
            &Default::default(),
            &Default::default(),
        ));
        assert!(names(&resolution).is_empty());
        assert_eq!(resolution.discoverable.len(), 1);
        // Metadata consumers must skip injection when the tool is absent.
        assert!(!resolution.general_enabled);
    }

    #[test]
    fn test_discoverable_outside_available_pool_disables_general() {
        // The discoverable config entry is not part of the available pool:
        // the assembled schema must not expose `general`, and the metadata
        // injection must not happen (single decision source).
        let registry = registry_with(vec![
            make_tool("a", "alpha", None),
            make_tool("d", "web_search", None),
            general_tool(),
        ]);
        let resolution = resolve_tool_exposure(input(
            &registry,
            &["alpha".to_string()],
            &[],
            &["web_search".to_string()],
            &[],
            &Default::default(),
            &Default::default(),
        ));
        assert_eq!(names(&resolution), vec!["alpha".to_string()]);
        assert!(resolution.discoverable.is_empty());
        assert!(!resolution.general_enabled);
    }

    #[test]
    fn test_hidden_discoverable_tool_is_filtered() {
        // A discoverable config entry listed in the hidden blocklist never
        // reaches the metadata bucket and never enables `general`.
        let registry = registry_with(vec![
            make_tool("a", "alpha", None),
            make_tool("d", "web_search", None),
            general_tool(),
        ]);
        let resolution = resolve_tool_exposure(input(
            &registry,
            &["alpha".to_string(), "web_search".to_string()],
            &[],
            &["web_search".to_string()],
            &["web_search".to_string()],
            &Default::default(),
            &Default::default(),
        ));
        assert_eq!(names(&resolution), vec!["alpha".to_string()]);
        assert!(resolution.discoverable.is_empty());
        assert_eq!(resolution.hidden.len(), 1);
        assert!(!resolution.general_enabled);
    }

    #[test]
    fn test_hidden_never_appears() {
        let registry = registry_with(vec![
            make_tool("a", "alpha", None),
            make_tool("h", "secret_admin", Some(ToolExposure::Hidden)),
        ]);
        let resolution = resolve_tool_exposure(input(
            &registry,
            &[],
            &[],
            &[],
            &[],
            &Default::default(),
            &Default::default(),
        ));
        assert_eq!(names(&resolution), vec!["alpha".to_string()]);
        assert!(resolution.discoverable.is_empty());
        assert!(resolution.gated.is_empty());
        assert_eq!(resolution.hidden.len(), 1);
    }

    #[test]
    fn test_whitelist_filters_visible_tools() {
        let registry = registry_with(vec![
            make_tool("a", "alpha", None),
            make_tool("b", "beta", None),
            make_tool("c", "gamma", None),
        ]);
        let resolution = resolve_tool_exposure(input(
            &registry,
            &["beta".to_string()],
            &[],
            &[],
            &[],
            &Default::default(),
            &Default::default(),
        ));
        assert_eq!(names(&resolution), vec!["beta".to_string()]);
    }

    #[test]
    fn test_blocked_tool_stays_in_schema() {
        // block is a runtime interception only; the schema is unchanged.
        let registry = registry_with(vec![make_tool("a", "alpha", None)]);
        let resolution = resolve_tool_exposure(input(
            &registry,
            &[],
            &[],
            &[],
            &[],
            &Default::default(),
            &Default::default(),
        ));
        assert_eq!(names(&resolution), vec!["alpha".to_string()]);
    }

    #[test]
    fn test_explicit_hidden_list_supplements_metadata() {
        let registry = registry_with(vec![
            make_tool("a", "alpha", None),
            make_tool("b", "beta", None),
        ]);
        let resolution = resolve_tool_exposure(input(
            &registry,
            &[],
            &[],
            &[],
            &["beta".to_string()],
            &Default::default(),
            &Default::default(),
        ));
        assert_eq!(names(&resolution), vec!["alpha".to_string()]);
        assert_eq!(resolution.hidden.len(), 1);
    }

    #[test]
    fn test_metadata_discoverable_promoted_to_list() {
        let registry = registry_with(vec![
            make_tool("a", "alpha", None),
            make_tool("d", "beta_db", Some(ToolExposure::Discoverable)),
        ]);
        let resolution = resolve_tool_exposure(input(
            &registry,
            &[],
            &[],
            &[],
            &[],
            &Default::default(),
            &Default::default(),
        ));
        assert_eq!(names(&resolution), vec!["alpha".to_string()]);
        let discoverable: Vec<String> = resolution
            .discoverable
            .iter()
            .map(|t| t.name.clone())
            .collect();
        assert_eq!(discoverable, vec!["beta_db".to_string()]);
    }

    #[test]
    fn test_exposure_override_switches_tool_set() {
        let registry = registry_with(vec![
            make_tool("a", "alpha", None),
            make_tool("d", "beta_db", Some(ToolExposure::Discoverable)),
        ]);
        // Reviewer form: promote the discoverable tool and demote alpha.
        let mut overrides = HashMap::new();
        overrides.insert("beta_db".to_string(), ToolExposure::Direct);
        overrides.insert("alpha".to_string(), ToolExposure::Discoverable);
        let resolution = resolve_tool_exposure(input(
            &registry,
            &[],
            &[],
            &[],
            &[],
            &overrides,
            &Default::default(),
        ));
        assert_eq!(names(&resolution), vec!["beta_db".to_string()]);
        assert_eq!(resolution.discoverable.len(), 1);
    }

    #[test]
    fn test_direct_model_only_in_visible_set() {
        let registry = registry_with(vec![make_tool(
            "m",
            "model_only",
            Some(ToolExposure::DirectModelOnly),
        )]);
        let resolution = resolve_tool_exposure(input(
            &registry,
            &[],
            &[],
            &[],
            &[],
            &Default::default(),
            &Default::default(),
        ));
        assert_eq!(names(&resolution), vec!["model_only".to_string()]);
    }

    #[test]
    fn test_is_tool_callable_buckets() {
        let registry = registry_with(vec![
            make_tool("a", "alpha", None),
            make_tool("d", "web_search", None),
            make_tool("g", "write_file", None),
            make_tool("h", "secret_admin", Some(ToolExposure::Hidden)),
            general_tool(),
        ]);
        let resolution = resolve_tool_exposure(input(
            &registry,
            &[
                "alpha".to_string(),
                "web_search".to_string(),
                "write_file".to_string(),
            ],
            &["alpha".to_string()],
            &["web_search".to_string()],
            &[],
            &Default::default(),
            &Default::default(),
        ));
        // visible: alpha, general; discoverable: web_search; gated: write_file; hidden: secret_admin.
        assert!(is_tool_callable(&resolution, "alpha"));
        assert!(is_tool_callable(&resolution, "web_search"));
        assert!(is_tool_callable(&resolution, "general"));
        assert!(!is_tool_callable(&resolution, "write_file"));
        assert!(!is_tool_callable(&resolution, "secret_admin"));
        assert!(!is_tool_callable(&resolution, "not_registered"));
    }

    #[test]
    fn test_is_tool_callable_metadata_discoverable() {
        // A tool whose discoverability comes from metadata exposure (not the
        // config list) must be callable via `general`: the schema assembly
        // injects its metadata, so the runtime gate has to agree.
        let registry = registry_with(vec![
            make_tool("a", "alpha", None),
            make_tool("d", "beta_db", Some(ToolExposure::Discoverable)),
            general_tool(),
        ]);
        let resolution = resolve_tool_exposure(input(
            &registry,
            &["alpha".to_string(), "beta_db".to_string()],
            &["alpha".to_string()],
            &[],
            &[],
            &Default::default(),
            &Default::default(),
        ));
        assert!(resolution.general_enabled);
        assert_eq!(resolution.discoverable.len(), 1);
        assert!(is_tool_callable(&resolution, "beta_db"));
    }

    #[test]
    fn test_is_tool_callable_activation_promotes_gated() {
        let registry = registry_with(vec![
            make_tool("a", "alpha", None),
            make_tool("g", "write_file", None),
            general_tool(),
        ]);
        let activated: HashSet<String> = ["write_file".to_string()].into_iter().collect();
        let resolution = resolve_tool_exposure(input(
            &registry,
            &["alpha".to_string(), "write_file".to_string()],
            &["alpha".to_string()],
            &[],
            &[],
            &Default::default(),
            &activated,
        ));
        assert!(is_tool_callable(&resolution, "write_file"));
    }
}
