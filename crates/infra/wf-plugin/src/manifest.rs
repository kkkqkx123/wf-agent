//! Manifest contract moved to `wf-plugin-sdk`; this module re-exports it so
//! existing `wf_plugin::manifest` / `wf_plugin::PluginManifest` paths stay
//! valid. It also owns the manifest-derived helpers that every backend
//! shares, so backend-agnostic code never has to reach into a backend
//! module for them.

pub use wf_plugin_sdk::manifest::{
    LuaConfig, PluginLlmProviderDefinition, PluginManifest, PluginModelDiscovery, PluginModelInfo,
    PluginPermission, PluginType, WasmConfig,
};

/// Single predicate for plugin version validity shared by admission and
/// dependency resolution.
pub fn is_valid_plugin_version(version: &str) -> bool {
    semver::Version::parse(version).is_ok()
}

/// Every artifact path a plugin may load: the default entry plus every
/// versioned entry, in manifest order and without duplicates.
///
/// Backend-agnostic on purpose: signature verification covers every
/// candidate artifact of a multi-version plugin, and the package manager
/// verifies artifacts of all backends. Lives here rather than in a backend
/// module so those paths stay reachable in builds that compile out a
/// backend.
pub fn all_entry_points(manifest: &PluginManifest) -> Vec<String> {
    let mut entries = vec![manifest.entry_point.clone()];
    for entry in manifest.versioned_entry_points.values() {
        if !entries.contains(entry) {
            entries.push(entry.clone());
        }
    }
    entries
}

/// Register manifest-declared `tools` / `tool_descriptions` into the
/// contribution manager. Entries are `Tool` / `ToolDescriptionData` JSON;
/// malformed entries fail loudly. Called by every backend after guest
/// declarations so manifest and guest surfaces share one registry path.
pub fn register_manifest_tools(
    manifest: &PluginManifest,
    registrar: &mut dyn crate::contributions::registrar::ContributionRegistrar,
) -> crate::error::PluginResult<()> {
    for raw in &manifest.tools {
        let tool: wf_types::tool::Tool = serde_json::from_value(raw.clone()).map_err(|e| {
            crate::error::PluginError::InvalidContribution {
                plugin_id: manifest.id.clone(),
                message: format!("invalid manifest tool JSON: {e}"),
            }
        })?;
        let id = tool.id.clone();
        registrar.register_tool(&id, tool)?;
    }
    for raw in &manifest.tool_descriptions {
        let description: wf_types::tool_description::ToolDescriptionData =
            serde_json::from_value(raw.clone()).map_err(|e| {
                crate::error::PluginError::InvalidContribution {
                    plugin_id: manifest.id.clone(),
                    message: format!("invalid manifest tool-description JSON: {e}"),
                }
            })?;
        let id = description.id.clone();
        registrar.register_tool_description(&id, description)?;
    }
    Ok(())
}
