//! Unified trigger handling at the composition boundary.
//!
//! Trigger templates fire asynchronously from the runtime listener. The only
//! composition concern is which registered templates are active: an absent
//! `enabled` flag means enabled, matching the hook convention.

use wf_core::registry::Registry;
use wf_resource::registry::ResourceRegistries;
use wf_types::trigger::TriggerTemplate;

/// Whether a trigger template participates in dispatch.
pub fn is_active(template: &TriggerTemplate) -> bool {
    template.enabled.unwrap_or(true)
}

/// Resolve one trigger template by name, returning `None` for unknown names.
pub fn resolve_trigger(regs: &ResourceRegistries, name: &str) -> Option<TriggerTemplate> {
    regs.trigger_templates.get(name).map(|t| t.as_ref().clone())
}

/// Active trigger templates in deterministic contract order.
pub fn active_trigger_templates(regs: &ResourceRegistries) -> Vec<TriggerTemplate> {
    let mut active: Vec<TriggerTemplate> = regs
        .trigger_templates
        .list()
        .iter()
        .filter_map(|key| regs.trigger_templates.get(key).map(|t| t.as_ref().clone()))
        .filter(is_active)
        .collect();
    wf_types::trigger::sort_winners_deterministically(&mut active);
    active
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_core::registry::MutableRegistry;

    fn template(name: &str, enabled: Option<bool>) -> TriggerTemplate {
        TriggerTemplate {
            name: name.into(),
            description: None,
            condition: None,
            action: None,
            enabled,
            max_triggers: None,
            priority: None,
            dispatch_mode: None,
            allow_multi_effect: None,
            effect_order: None,
            metadata: None,
            created_at: 0,
            updated_at: 0,
            create_checkpoint: None,
            checkpoint_description_template: None,
        }
    }

    #[test]
    fn absent_enabled_means_active() {
        assert!(is_active(&template("a", None)));
        assert!(is_active(&template("b", Some(true))));
        assert!(!is_active(&template("c", Some(false))));
    }

    #[test]
    fn only_active_templates_resolve() {
        let regs = ResourceRegistries::new();
        regs.trigger_templates
            .register("on".into(), std::sync::Arc::new(template("on", None)))
            .expect("register");
        regs.trigger_templates
            .register(
                "off".into(),
                std::sync::Arc::new(template("off", Some(false))),
            )
            .expect("register");
        assert!(resolve_trigger(&regs, "on").is_some());
        assert!(resolve_trigger(&regs, "missing").is_none());
        let active = active_trigger_templates(&regs);
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].name, "on");
    }
}
