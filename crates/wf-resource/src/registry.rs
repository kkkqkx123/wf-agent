use std::sync::Arc;

use std::path::Path;

use wf_core::registry::{ConcurrentRegistry, MutableRegistry, Registry};
use wf_tools::registry::ToolRegistry;
use wf_types::agent::AgentTemplate;
use wf_types::tool_description::ToolDescriptionData;
use wf_types::trigger::TriggerTemplate;
use wf_types::workflow::{NodeTemplate, WorkflowTemplate};
use wf_types::{SystemPromptFragment, Template};

use crate::custom;
use crate::predefined;
use crate::result::Summary;

pub static PREDEFINED_FRAGMENT_IDS: &[&str] = &[
    "fragments.role.assistant",
    "fragments.role.coder",
    "fragments.role.analyst",
    "fragments.capability.general",
    "fragments.capability.general-principles",
    "fragments.capability.coding",
    "fragments.constraint.general",
    "fragments.constraint.general-interaction",
    "fragments.constraint.coding",
    "fragments.constraint.code-safety",
    "fragments.tool-usage.xml-summary",
    "fragments.tool-usage.json-summary",
    "fragments.task-instruction.code-review",
    "fragments.task-instruction.data-analysis",
];

pub static PREDEFINED_PROMPT_IDS: &[&str] = &["system.default", "system.code", "system.agent"];

/// Ids of all predefined tool descriptions, derived from the single-source
/// tool definitions in wf-tools.
pub fn predefined_tool_description_ids() -> Vec<&'static str> {
    wf_tools::predefined::all_tool_ids()
}

#[derive(Debug, Clone)]
pub struct ResourcePluginActivation {
    pub id: String,
    pub config: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct RegisterOptions {
    pub skip_if_exists: bool,
    pub allow_list: Option<Vec<String>>,
    pub block_list: Option<Vec<String>>,
    pub custom_resources: Option<crate::custom::types::CustomResourcesPresetConfig>,
    pub custom_base_dir: Option<String>,
    pub resource_plugin_activation: Vec<ResourcePluginActivation>,
}

impl Default for RegisterOptions {
    fn default() -> Self {
        Self {
            skip_if_exists: true,
            allow_list: None,
            block_list: None,
            custom_resources: None,
            custom_base_dir: None,
            resource_plugin_activation: Vec::new(),
        }
    }
}

pub struct ResourceRegistries {
    pub fragments: ConcurrentRegistry<SystemPromptFragment>,
    /// Unified templateable prompt texts: system-prompt templates
    /// (`system.default` et al.) plus tool-visibility texts (activation/block
    /// announcements, discoverable metadata block, general description).
    pub templates: ConcurrentRegistry<Template>,
    pub tool_descriptions: ConcurrentRegistry<ToolDescriptionData>,
    pub trigger_templates: ConcurrentRegistry<TriggerTemplate>,
    pub workflows: ConcurrentRegistry<WorkflowTemplate>,
    pub node_templates: ConcurrentRegistry<NodeTemplate>,
    pub agent_templates: ConcurrentRegistry<AgentTemplate>,
}

impl ResourceRegistries {
    pub fn new() -> Self {
        Self {
            fragments: ConcurrentRegistry::new(),
            templates: ConcurrentRegistry::new(),
            tool_descriptions: ConcurrentRegistry::new(),
            trigger_templates: ConcurrentRegistry::new(),
            workflows: ConcurrentRegistry::new(),
            node_templates: ConcurrentRegistry::new(),
            agent_templates: ConcurrentRegistry::new(),
        }
    }
}

impl Default for ResourceRegistries {
    fn default() -> Self {
        Self::new()
    }
}

/// Register an item into a registry, skipping if the key already exists.
///
/// When `skip_if_exists` is true, existing keys are silently skipped (no-op).
/// When `skip_if_exists` is false, duplicate keys are reported as failures.
#[deprecated(since = "0.1.0", note = "use `register_item` without skip_if_exists, or call `registry.has()` first")]
pub fn register_item<T: Send + Sync>(
    registry: &ConcurrentRegistry<T>,
    key: String,
    item: T,
    skip_if_exists: bool,
) -> Summary {
    register_item_inner(registry, key, item, skip_if_exists)
}

/// Register an item into a registry. Fails if the key already exists.
pub fn register_item_strict<T: Send + Sync>(
    registry: &ConcurrentRegistry<T>,
    key: String,
    item: T,
) -> Summary {
    register_item_inner(registry, key, item, false)
}

/// Register an item into a registry, silently skipping existing keys.
pub fn register_item_skip<T: Send + Sync>(
    registry: &ConcurrentRegistry<T>,
    key: String,
    item: T,
) -> Summary {
    register_item_inner(registry, key, item, true)
}

fn register_item_inner<T: Send + Sync>(
    registry: &ConcurrentRegistry<T>,
    key: String,
    item: T,
    skip_if_exists: bool,
) -> Summary {
    if skip_if_exists && registry.has(&key) {
        return Summary::ok(&key);
    }

    let item = Arc::new(item);
    match registry.register(key.clone(), item) {
        Ok(()) => Summary::ok(key),
        Err(e) => Summary::err(key, e.to_string()),
    }
}

/// Validate and register a single prompt template: invalid templates are
/// reported as failures instead of being silently stored.
pub fn register_template(
    regs: &ResourceRegistries,
    template: Template,
    skip_if_exists: bool,
) -> Summary {
    let id = template.id.clone();
    if let Err(e) = wf_config::processor::prompt::validate_prompt_template(&template) {
        return Summary::err(&id, e.to_string());
    }
    if skip_if_exists {
        register_item_skip(&regs.templates, id, template)
    } else {
        register_item_strict(&regs.templates, id, template)
    }
}

/// Validate and register a single system-prompt fragment (category must be
/// one of `wf_types::FRAGMENT_CATEGORIES`).
pub fn register_fragment(
    regs: &ResourceRegistries,
    fragment: SystemPromptFragment,
    skip_if_exists: bool,
) -> Summary {
    let id = fragment.id.clone();
    let error = if fragment.id.trim().is_empty() {
        Some("fragment id must not be empty".to_string())
    } else if fragment.content.trim().is_empty() {
        Some(format!("fragment '{}' has empty content", fragment.id))
    } else if !wf_types::is_valid_fragment_category(&fragment.category) {
        Some(format!(
            "invalid fragment category '{}' (allowed: {})",
            fragment.category,
            wf_types::FRAGMENT_CATEGORIES.join(", ")
        ))
    } else {
        None
    };
    if let Some(e) = error {
        return Summary::err(&id, e);
    }
    if skip_if_exists {
        register_item_skip(&regs.fragments, id, fragment)
    } else {
        register_item_strict(&regs.fragments, id, fragment)
    }
}

/// Template ids whose declared fragment list references `fragment_id`
/// (dependency tracking for cascade-safe unregistration).
pub fn templates_depending_on_fragment(regs: &ResourceRegistries, fragment_id: &str) -> Vec<String> {
    regs.templates
        .list()
        .into_iter()
        .filter(|id| {
            regs.templates
                .get(id)
                .map(|t| {
                    t.fragments
                        .as_ref()
                        .map(|fragments| fragments.iter().any(|f| f == fragment_id))
                        .unwrap_or(false)
                })
                .unwrap_or(false)
        })
        .collect()
}

/// Unregister a fragment with cascade protection: when any registered
/// template still references the fragment, the request is rejected with the
/// dependent template ids so compositions cannot silently break.
pub fn unregister_fragment_checked(regs: &ResourceRegistries, id: &str) -> Result<(), String> {
    let dependents = templates_depending_on_fragment(regs, id);
    if !dependents.is_empty() {
        return Err(format!(
            "fragment '{}' is still referenced by templates: {}",
            id,
            dependents.join(", ")
        ));
    }
    regs.fragments
        .unregister(id)
        .map(|_| ())
        .ok_or_else(|| format!("fragment not found: {}", id))
}

/// Unregister a prompt template (templates have no dependents; fragments
/// they reference stay registered for other templates).
pub fn unregister_template(regs: &ResourceRegistries, id: &str) -> Summary {
    match regs.templates.unregister(id) {
        Some(_) => Summary::ok(id),
        None => Summary::err(id, format!("template not found: {}", id)),
    }
}

/// Template ids registered under `category`, sorted for stable output.
pub fn list_templates_by_category(regs: &ResourceRegistries, category: &str) -> Vec<String> {
    let mut ids: Vec<String> = regs
        .templates
        .list()
        .into_iter()
        .filter(|id| {
            regs.templates
                .get(id)
                .map(|t| t.category == category)
                .unwrap_or(false)
        })
        .collect();
    ids.sort();
    ids
}

/// Fragment ids registered under `category`, sorted for stable output.
pub fn list_fragments_by_category(regs: &ResourceRegistries, category: &str) -> Vec<String> {
    let mut ids: Vec<String> = regs
        .fragments
        .list()
        .into_iter()
        .filter(|id| {
            regs.fragments
                .get(id)
                .map(|f| f.category == category)
                .unwrap_or(false)
        })
        .collect();
    ids.sort();
    ids
}

/// Register the predefined and config-driven custom resources into the
/// runtime registries.
///
/// Built-in resource plugins are **not activated here**: the runtime's
/// plugin engine registers them as `wf-plugin` plugins and activates them
/// through the contribution bridge, so the whole plugin system shares one
/// activation chain. The legacy `ResourcePluginRegistry` path remains
/// available for engine-disabled fallbacks.
pub fn register_all(
    regs: &ResourceRegistries,
    tool_registry: &ToolRegistry,
    opts: &RegisterOptions,
) -> Summary {
    let mut total = Summary::new();

    // Predefined resources
    total.merge(predefined::fragments::register(regs, opts));
    total.merge(predefined::prompts::register(regs, opts));
    total.merge(predefined::tool_descriptions::register(regs, opts));
    total.merge(predefined::agent_templates::register(regs, opts));
    total.merge(predefined::tools::register(tool_registry, opts));
    total.merge(predefined::workflow::register(regs, opts));
    total.merge(predefined::tool_visibility::register(regs, opts));

    // Custom resources (from config)
    if let Some(ref custom_config) = opts.custom_resources {
        let base_dir_str = opts.custom_base_dir.as_deref().unwrap_or(".");
        let base_dir = Path::new(base_dir_str);
        let resources = custom::loader::load_custom_resources(custom_config, base_dir);
        let validation_level = custom_config.validation_level.unwrap_or_default();
        total.merge(custom::register::register_custom_resources(
            regs,
            tool_registry,
            resources,
            opts.skip_if_exists,
            validation_level,
        ));
    }

    total
}

// ── helpers ──────────────────────────────────────────────

pub fn is_resource_disabled(id: &str, opts: &RegisterOptions) -> bool {
    if let Some(ref allow) = opts.allow_list {
        return !allow.contains(&id.to_string());
    }
    if let Some(ref block) = opts.block_list {
        return block.contains(&id.to_string());
    }
    false
}

pub fn are_fragments_registered(regs: &ResourceRegistries) -> bool {
    PREDEFINED_FRAGMENT_IDS
        .iter()
        .all(|id| regs.fragments.has(id))
}

pub fn are_prompt_templates_registered(regs: &ResourceRegistries) -> bool {
    PREDEFINED_PROMPT_IDS
        .iter()
        .all(|id| regs.templates.has(id))
}

pub fn are_predefined_tool_descriptions_registered(regs: &ResourceRegistries) -> bool {
    predefined_tool_description_ids()
        .iter()
        .all(|id| regs.tool_descriptions.has(id))
}

pub fn unregister_predefined_content(regs: &ResourceRegistries) -> Summary {
    let mut total = Summary::new();

    for id in PREDEFINED_FRAGMENT_IDS {
        if regs.fragments.unregister(id).is_some() {
            total.merge(Summary::ok(*id));
        }
    }
    for id in PREDEFINED_PROMPT_IDS {
        if regs.templates.unregister(id).is_some() {
            total.merge(Summary::ok(*id));
        }
    }
    for id in predefined_tool_description_ids() {
        if regs.tool_descriptions.unregister(id).is_some() {
            total.merge(Summary::ok(id));
        }
    }

    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_core::registry::Registry;
    use wf_types::{SystemPromptFragment, Template};

    fn fragment(id: &str, category: &str) -> SystemPromptFragment {
        SystemPromptFragment {
            id: id.into(),
            category: category.into(),
            content: "content".into(),
            description: None,
            variables: None,
        }
    }

    fn template(id: &str, category: &str, fragments: Option<Vec<String>>) -> Template {
        Template {
            id: id.into(),
            name: id.into(),
            description: None,
            category: category.into(),
            content: "Body {{fragments}}".into(),
            variables: None,
            fragments,
        }
    }

    #[test]
    fn register_template_rejects_invalid_category() {
        let regs = ResourceRegistries::new();
        let summary = register_template(&regs, template("t.bad", "nope", None), false);
        assert!(!summary.failed.is_empty());
        assert!(!regs.templates.has("t.bad"));
    }

    #[test]
    fn register_template_accepts_valid_category() {
        let regs = ResourceRegistries::new();
        let summary = register_template(&regs, template("t.ok", "system", None), false);
        assert!(summary.failed.is_empty(), "{:?}", summary.failed);
        assert!(regs.templates.has("t.ok"));
    }

    #[test]
    fn register_fragment_rejects_invalid_category() {
        let regs = ResourceRegistries::new();
        let summary = register_fragment(&regs, fragment("f.bad", "misc"), false);
        assert!(!summary.failed.is_empty());
        assert!(!regs.fragments.has("f.bad"));
    }

    #[test]
    fn register_fragment_rejects_empty_content() {
        let regs = ResourceRegistries::new();
        let mut empty = fragment("f.empty", "role");
        empty.content = "   ".into();
        let summary = register_fragment(&regs, empty, false);
        assert!(!summary.failed.is_empty());
    }

    #[test]
    fn list_by_category_returns_sorted_ids() {
        let regs = ResourceRegistries::new();
        register_fragment(&regs, fragment("fragments.role.coder", "role"), false);
        register_fragment(&regs, fragment("fragments.role.assistant", "role"), false);
        register_fragment(&regs, fragment("fragments.capability.general", "capability"), false);

        assert_eq!(
            list_fragments_by_category(&regs, "role"),
            vec!["fragments.role.assistant", "fragments.role.coder"]
        );
        assert_eq!(
            list_fragments_by_category(&regs, "capability"),
            vec!["fragments.capability.general"]
        );
        assert!(list_fragments_by_category(&regs, "task-instruction").is_empty());
    }

    #[test]
    fn fragment_dependency_tracking_blocks_cascade_uninstall() {
        let regs = ResourceRegistries::new();
        register_fragment(&regs, fragment("fragments.role.assistant", "role"), false);
        let summary = register_template(
            &regs,
            template(
                "system.custom",
                "system",
                Some(vec!["fragments.role.assistant".into()]),
            ),
            false,
        );
        assert!(summary.failed.is_empty(), "{:?}", summary.failed);

        // Dependency tracking finds the referencing template.
        assert_eq!(
            templates_depending_on_fragment(&regs, "fragments.role.assistant"),
            vec!["system.custom".to_string()]
        );

        // Uninstalling the referenced fragment is rejected.
        let err = unregister_fragment_checked(&regs, "fragments.role.assistant")
            .expect_err("dependent template must block uninstall");
        assert!(err.contains("system.custom"), "{}", err);
        assert!(regs.fragments.has("fragments.role.assistant"));

        // After removing the template the fragment uninstalls cleanly.
        unregister_template(&regs, "system.custom");
        unregister_fragment_checked(&regs, "fragments.role.assistant").expect("unblocked");
        assert!(!regs.fragments.has("fragments.role.assistant"));
    }

    #[test]
    fn unregister_fragment_checked_reports_missing() {
        let regs = ResourceRegistries::new();
        let err =
            unregister_fragment_checked(&regs, "nope").expect_err("missing fragment must error");
        assert!(err.contains("not found"), "{}", err);
    }

    #[test]
    fn list_templates_by_category_filters() {
        let regs = ResourceRegistries::new();
        register_template(&regs, template("system.one", "system", None), false);
        register_template(&regs, template("user.two", "user", None), false);
        assert_eq!(list_templates_by_category(&regs, "system"), vec!["system.one"]);
        assert!(list_templates_by_category(&regs, "rules").is_empty());
    }
}
