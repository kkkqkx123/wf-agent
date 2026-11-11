use wf_plugin::ContributionManager;

pub(crate) fn sync_images(plugin_id: &str, manager: &ContributionManager) {
    for (name, owner) in manager.all_llm_images() {
        if owner == plugin_id {
            tracing::debug!("  llm-images (plugin-owned): {}", name);
        }
    }
}

pub(crate) fn unsync_images(plugin_id: &str, manager: &ContributionManager) {
    for (name, owner) in manager.all_llm_images() {
        if owner == plugin_id {
            tracing::debug!("  llm-images (plugin-removed): {}", name);
        }
    }
}
