//! Single-source per-turn tool exposure resolution.
//!
//! [`resolve_tool_exposure`] computes the tool buckets (visible /
//! discoverable / gated / hidden) and the `general` tool exposure decision
//! from one shared set of inputs. Both the per-turn schema assembly
//! (wf-agent `ToolRouter`) and the agent-loop discoverable metadata
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

/// Closed-world available pool for one exposure resolution.
///
/// Unset inherits all registered tools; an explicit list (even empty) is
/// the closed whitelist, so an explicit empty pool exposes no tools
/// instead of inheriting everything. The enum keeps the distinction at
/// the type level so no call site can mistake an empty pool for unset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AvailablePool<'a> {
    Unset,
    Explicit(&'a [String]),
}

impl<'a> AvailablePool<'a> {
    pub fn from_option(names: Option<&'a [String]>) -> Self {
        match names {
            None => AvailablePool::Unset,
            Some(names) => AvailablePool::Explicit(names),
        }
    }

    pub fn is_unset(&self) -> bool {
        matches!(self, AvailablePool::Unset)
    }

    pub fn explicit_names(&self) -> Option<&'a [String]> {
        match self {
            AvailablePool::Unset => None,
            AvailablePool::Explicit(names) => Some(names),
        }
    }

    fn allows(&self, tool_name: &str) -> bool {
        match self {
            AvailablePool::Unset => true,
            AvailablePool::Explicit(names) => names.contains(&tool_name.to_string()),
        }
    }
}

/// Explicit pure chat run: explicit empty pool with no initial or
/// discoverable tools exposes nothing. Single predicate so every entry
/// path classifies pure chat identically instead of repeating the
/// three-list conjunction.
pub fn is_explicit_no_tools(
    available: AvailablePool<'_>,
    initial_names: &[String],
    discoverable_names: &[String],
) -> bool {
    matches!(available, AvailablePool::Explicit(names) if names.is_empty())
        && initial_names.is_empty()
        && discoverable_names.is_empty()
}

/// Whether the option pool is an explicit empty list (pure chat pool).
/// `None` (unset) never counts, even with empty initial/discoverable
/// lists. Used for contradiction checks where the question is only
/// whether the pool itself is explicitly empty.
pub fn is_explicit_empty_pool(available: Option<&[String]>) -> bool {
    matches!(
        AvailablePool::from_option(available),
        AvailablePool::Explicit(names) if names.is_empty()
    )
}

/// Option-based variant of [`is_explicit_no_tools`] for config-level
/// `Option<Vec<String>>` pools (`None` means unset, `Some` is explicit).
pub fn is_explicit_no_tools_option(
    available: Option<&[String]>,
    initial_names: &[String],
    discoverable_names: &[String],
) -> bool {
    is_explicit_no_tools(
        AvailablePool::from_option(available),
        initial_names,
        discoverable_names,
    )
}

/// Stable fingerprint of one tool declaration (name, description and
/// parameter schema). `serde_json` maps sort keys, so the text is
/// deterministic across processes, unlike the default hasher.
pub fn declaration_fingerprint(tool: &Tool) -> String {
    let params_text = tool
        .parameters
        .as_ref()
        .and_then(|params| serde_json::to_string(params).ok())
        .unwrap_or_default();
    let mut hasher = blake3::Hasher::new();
    hasher.update(tool.name.as_bytes());
    hasher.update(&[0]);
    hasher.update(tool.description.as_bytes());
    hasher.update(&[0]);
    hasher.update(params_text.as_bytes());
    hasher.update(&[0]);
    hasher.finalize().to_hex().to_string()
}

/// Stable fingerprint of a tool bucket. Tools sort by name first so
/// registry iteration order never leaks into the fingerprint.
pub fn fingerprint_declarations(tools: &[Tool]) -> String {
    let mut sorted: Vec<&Tool> = tools.iter().collect();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    let mut hasher = blake3::Hasher::new();
    hasher.update(&(sorted.len() as u64).to_le_bytes());
    for tool in sorted {
        hasher.update(declaration_fingerprint(tool).as_bytes());
        hasher.update(&[0]);
    }
    hasher.finalize().to_hex().to_string()
}

/// Inputs for one exposure resolution.
pub struct ExposureInput<'a> {
    pub registry: &'a ToolRegistry,
    /// Available pool for this resolution (unset inherits all registered
    /// tools, explicit is the closed whitelist).
    pub available: AvailablePool<'a>,
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
    let initial_empty = input.initial_names.is_empty();

    for tool in input.registry.list_tools() {
        if !input.available.allows(&tool.name) {
            continue;
        }
        // The `general` proxy is owned entirely by the injection block
        // below: it never sits in the gated bucket waiting for activation,
        // so the runtime gate and the schema cannot disagree about it.
        if tool.name == GENERAL_TOOL_NAME {
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

    // The `general` tool enters the visible set when discoverable tools
    // exist and it is not explicitly disabled. It is a member of the
    // declaration array, so a closed available pool must contain it to
    // allow injection; otherwise the proxy stays hidden and discoverable
    // tools are reported as temporarily unreachable by the caller. When the
    // tool is not registered, `general_enabled` falls back to false so
    // schema and metadata consumers share one decision.
    let mut general_enabled = input
        .enable_general_tool
        .unwrap_or(!discoverable.is_empty());
    if general_enabled
        && matches!(input.available, AvailablePool::Explicit(_))
        && !input.available.allows(GENERAL_TOOL_NAME)
    {
        general_enabled = false;
    }
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

/// Runtime gate for the direct-call path (model-emitted or replayed direct
/// invocations). Mirrors the bucket semantics the schema assembly uses:
/// hidden tools are never executable, gated tools require formal activation,
/// discoverable tools must go through the `general` proxy, and anything
/// outside the available pool is rejected. Visible tools (including the
/// `general` proxy itself) pass.
///
/// Inner `general` invocations must NOT use this gate: they are checked by
/// the proxy-side gate (`general` invoker), which permits discoverable
/// tools by design.
pub fn check_direct_tool_callable(
    resolution: &ExposureResolution,
    tool_name: &str,
) -> Result<(), String> {
    if resolution.hidden.iter().any(|t| t.name == tool_name) {
        return Err(format!(
            "Tool '{tool_name}' is not callable in this execution"
        ));
    }
    if resolution.gated.iter().any(|t| t.name == tool_name) {
        return Err(format!(
            "Tool '{tool_name}' is not activated yet; wait until it is explicitly enabled"
        ));
    }
    if resolution.discoverable.iter().any(|t| t.name == tool_name) {
        return Err(format!(
            "Tool '{tool_name}' is discoverable and must be invoked through the general tool"
        ));
    }
    if resolution.visible.iter().any(|t| t.name == tool_name) {
        return Ok(());
    }
    Err(format!(
        "Tool '{tool_name}' is not in the available tool set"
    ))
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
        available: AvailablePool<'a>,
        initial: &'a [String],
        discoverable: &'a [String],
        hidden: &'a [String],
        overrides: &'a HashMap<String, ToolExposure>,
        activated: &'a HashSet<String>,
    ) -> ExposureInput<'a> {
        ExposureInput {
            registry,
            available,
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
            AvailablePool::Unset,
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
            AvailablePool::Explicit(&["alpha".to_string(), "beta".to_string()]),
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
            AvailablePool::Explicit(&[
                "alpha".to_string(),
                "beta".to_string(),
                "gamma".to_string(),
            ]),
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
            AvailablePool::Explicit(&[
                "alpha".to_string(),
                "beta".to_string(),
                "gamma".to_string(),
            ]),
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
            AvailablePool::Explicit(&[
                "alpha".to_string(),
                "web_search".to_string(),
                "general".to_string(),
            ]),
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
    fn test_pool_without_general_disables_proxy() {
        let registry = registry_with(vec![
            make_tool("a", "alpha", None),
            make_tool("d", "web_search", None),
            general_tool(),
        ]);
        let resolution = resolve_tool_exposure(input(
            &registry,
            AvailablePool::Explicit(&["alpha".to_string(), "web_search".to_string()]),
            &["alpha".to_string()],
            &["web_search".to_string()],
            &[],
            &Default::default(),
            &Default::default(),
        ));
        assert!(!resolution.general_enabled);
        assert_eq!(names(&resolution), vec!["alpha".to_string()]);
        assert_eq!(resolution.discoverable.len(), 1);
    }

    #[test]
    fn test_explicit_empty_pool_exposes_nothing() {
        let registry = registry_with(vec![make_tool("a", "alpha", None), general_tool()]);
        let empty: Vec<String> = Vec::new();
        let resolution = resolve_tool_exposure(input(
            &registry,
            AvailablePool::Explicit(&empty),
            &[],
            &[],
            &[],
            &Default::default(),
            &Default::default(),
        ));
        assert!(resolution.visible.is_empty());
        assert!(resolution.discoverable.is_empty());
        assert!(resolution.gated.is_empty());
        assert!(!resolution.general_enabled);
    }

    #[test]
    fn test_declaration_fingerprint_is_stable_and_content_sensitive() {
        let first = fingerprint_declarations(&[
            make_tool("b", "beta", None),
            make_tool("a", "alpha", None),
        ]);
        let second = fingerprint_declarations(&[
            make_tool("a", "alpha", None),
            make_tool("b", "beta", None),
        ]);
        assert_eq!(first, second);
        let mut changed = make_tool("a", "alpha", None);
        changed.description = "changed".to_string();
        let third = fingerprint_declarations(&[changed, make_tool("b", "beta", None)]);
        assert_ne!(first, third);
        assert_eq!(
            declaration_fingerprint(&make_tool("a", "alpha", None)),
            declaration_fingerprint(&make_tool("a", "alpha", None))
        );
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
            AvailablePool::Explicit(&available),
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
            AvailablePool::Explicit(&["web_search".to_string()]),
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
            AvailablePool::Explicit(&["alpha".to_string()]),
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
            AvailablePool::Explicit(&["alpha".to_string(), "web_search".to_string()]),
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
            AvailablePool::Unset,
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
            AvailablePool::Explicit(&["beta".to_string()]),
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
            AvailablePool::Unset,
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
            AvailablePool::Unset,
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
            AvailablePool::Unset,
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
            AvailablePool::Unset,
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
            AvailablePool::Unset,
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
            AvailablePool::Explicit(&[
                "alpha".to_string(),
                "web_search".to_string(),
                "write_file".to_string(),
                "general".to_string(),
            ]),
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
    fn test_check_direct_tool_callable_buckets() {
        let registry = registry_with(vec![
            make_tool("a", "alpha", None),
            make_tool("d", "web_search", None),
            make_tool("g", "write_file", None),
            make_tool("h", "secret_admin", Some(ToolExposure::Hidden)),
            general_tool(),
        ]);
        let resolution = resolve_tool_exposure(input(
            &registry,
            AvailablePool::Explicit(&[
                "alpha".to_string(),
                "web_search".to_string(),
                "write_file".to_string(),
                "secret_admin".to_string(),
                "general".to_string(),
            ]),
            &["alpha".to_string()],
            &["web_search".to_string()],
            &[],
            &Default::default(),
            &Default::default(),
        ));
        // visible: alpha, general; discoverable: web_search; gated:
        // write_file; hidden: secret_admin.
        assert!(check_direct_tool_callable(&resolution, "alpha").is_ok());
        assert!(check_direct_tool_callable(&resolution, "general").is_ok());
        assert_eq!(
            check_direct_tool_callable(&resolution, "web_search").unwrap_err(),
            "Tool 'web_search' is discoverable and must be invoked through the general tool"
        );
        assert!(check_direct_tool_callable(&resolution, "write_file")
            .unwrap_err()
            .contains("not activated"));
        assert!(check_direct_tool_callable(&resolution, "secret_admin")
            .unwrap_err()
            .contains("not callable"));
        assert!(check_direct_tool_callable(&resolution, "not_registered")
            .unwrap_err()
            .contains("not in the available tool set"));
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
            AvailablePool::Explicit(&[
                "alpha".to_string(),
                "beta_db".to_string(),
                "general".to_string(),
            ]),
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
            AvailablePool::Explicit(&["alpha".to_string(), "write_file".to_string()]),
            &["alpha".to_string()],
            &[],
            &[],
            &Default::default(),
            &activated,
        ));
        assert!(is_tool_callable(&resolution, "write_file"));
    }
}
