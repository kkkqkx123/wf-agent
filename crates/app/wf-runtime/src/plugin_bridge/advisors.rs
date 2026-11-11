use wf_plugin::{ContributionManager, PluginError, PluginResult};

pub(crate) fn sync_advisors(plugin_id: &str, manager: &ContributionManager) -> PluginResult<()> {
    for (name, owner) in manager.all_compaction_advisors() {
        if owner != plugin_id {
            continue;
        }
        let Some(advisor) = manager.get_compaction_advisor(&name) else {
            continue;
        };
        if !wf_execution_shared::register_compaction_advisor(&name, advisor) {
            return Err(PluginError::LoadFailed(format!(
                "plugin '{plugin_id}' compaction advisor '{name}' conflicts with an existing strategy"
            )));
        }
        tracing::debug!("  compaction-advisor (engine-registered): {}", name);
    }
    Ok(())
}

pub(crate) fn unsync_advisors(plugin_id: &str, manager: &ContributionManager) {
    for (name, owner) in manager.all_compaction_advisors() {
        if owner == plugin_id {
            wf_execution_shared::unregister_compaction_advisor(&name);
            tracing::debug!("  compaction-advisor (engine-removed): {}", name);
        }
    }
}
