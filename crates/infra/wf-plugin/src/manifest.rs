//! Manifest contract moved to `wf-plugin-sdk`; this module re-exports it so
//! existing `wf_plugin::manifest` / `wf_plugin::PluginManifest` paths stay
//! valid.

pub use wf_plugin_sdk::manifest::{PluginManifest, PluginPermission, PluginType};
