use std::sync::{OnceLock, RwLock};

use wf_types::config::limits::CompactionLimits;

/// Built-in compression strategy name. Empty config normalizes here.
pub const BUILTIN_COMPRESSION_STRATEGY_NAME: &str = "builtin";
/// Built-in single-summary mode name. Selects the legacy single summary
/// without layered blocks: thresholds still gate emission, but no block
/// planning, re-compression or block-based handoff applies.
pub const SINGLE_SUMMARY_STRATEGY_NAME: &str = "single-summary";

/// Effective strategy name for compaction limits. Trims the configured
/// value; absent or blank means the built-in strategy.
pub fn compaction_strategy_name(limits: &CompactionLimits) -> String {
    limits
        .strategy
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(BUILTIN_COMPRESSION_STRATEGY_NAME)
        .to_string()
}

/// Whether a strategy name selects the built-in planner and chains.
pub fn is_builtin_strategy(name: &str) -> bool {
    let trimmed = name.trim();
    trimmed.is_empty() || trimmed == BUILTIN_COMPRESSION_STRATEGY_NAME
}

/// Whether a strategy name selects the legacy single summary without
/// layered blocks. The engine still owns thresholds, version anchors
/// and write-back; planning and block-based handoff are bypassed.
pub fn is_single_summary_strategy(name: &str) -> bool {
    name.trim() == SINGLE_SUMMARY_STRATEGY_NAME
}

/// Version of the advisor planning protocol. Sent with every advisor
/// input so future protocol evolution can branch on it.
pub const ADVISOR_PROTOCOL_VERSION: u32 = 1;

/// Candidate summary workflow ids for a strategy name, before registry
/// lookup. Built-in names keep the fold-summary chains; any other name
/// resolves to the strategy id with `-repress` suffixed for the
/// re-compression chain when present.
pub fn strategy_workflow_ids(strategy: &str) -> (String, Option<String>) {
    if is_builtin_strategy(strategy) || is_single_summary_strategy(strategy) {
        (
            wf_resource::predefined::workflow::FOLD_SUMMARY_WORKFLOW_ID.to_string(),
            Some(wf_resource::predefined::workflow::FOLD_SUMMARY_REPRESS_WORKFLOW_ID.to_string()),
        )
    } else {
        let trimmed = strategy.trim().to_string();
        let repress = format!("{trimmed}-repress");
        (trimmed, Some(repress))
    }
}

/// Process-wide compaction advisor registry backing named strategies.
/// Advisors are pure shaping hooks contributed by plugins; the engine
/// validates every suggestion and falls back to the built-in planner.
static COMPACTION_ADVISORS: OnceLock<
    dashmap::DashMap<String, std::sync::Arc<dyn wf_plugin_sdk::PluginCompactionAdvisor>>,
> = OnceLock::new();

fn compaction_advisors(
) -> &'static dashmap::DashMap<String, std::sync::Arc<dyn wf_plugin_sdk::PluginCompactionAdvisor>> {
    COMPACTION_ADVISORS.get_or_init(dashmap::DashMap::new)
}

/// Register planning advice for a named strategy. Returns false when the
/// name is already taken; unregister first to replace it.
pub fn register_compaction_advisor(
    name: &str,
    advisor: std::sync::Arc<dyn wf_plugin_sdk::PluginCompactionAdvisor>,
) -> bool {
    let key = name.trim().to_string();
    if key.is_empty() || compaction_advisors().contains_key(&key) {
        return false;
    }
    compaction_advisors().insert(key, advisor);
    true
}

/// Remove planning advice for a named strategy.
pub fn unregister_compaction_advisor(name: &str) -> bool {
    compaction_advisors().remove(name.trim()).is_some()
}

/// Planning advice for a named strategy, if registered.
pub fn get_compaction_advisor(
    name: &str,
) -> Option<std::sync::Arc<dyn wf_plugin_sdk::PluginCompactionAdvisor>> {
    compaction_advisors().get(name.trim()).map(|e| e.clone())
}

static PROCESS_LIMITS: RwLock<Option<CompactionLimits>> = RwLock::new(None);

/// Inject the process-wide compaction parameters at bootstrap, mirroring
/// the compression settle-budget injection. Later installs overwrite
/// earlier ones so tests and re-bootstrap observe the newest config.
pub fn set_process_compaction_limits(limits: CompactionLimits) {
    *PROCESS_LIMITS
        .write()
        .expect("compaction limits lock poisoned") = Some(limits);
}

/// Effective process-wide compaction parameters. Bootstrap must install
/// them before any execution emits; missing installation fails loudly
/// instead of silently falling back to built-in defaults.
pub fn process_compaction_limits() -> CompactionLimits {
    PROCESS_LIMITS
        .read()
        .expect("compaction limits lock poisoned")
        .clone()
        .expect("compaction limits not installed")
}

/// Resolve the effective compaction tuning: an explicit injection wins,
/// otherwise the process-wide install. None keeps the global fallback
/// explicit at the call site so later explicit plumbing can tighten here.
pub fn resolve_effective_compaction_limits(
    explicit: Option<CompactionLimits>,
) -> CompactionLimits {
    explicit.unwrap_or_else(process_compaction_limits)
}
