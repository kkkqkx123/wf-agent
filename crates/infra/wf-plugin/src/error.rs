//! Error contract moved to `wf-plugin-sdk`; this module re-exports it so
//! existing `wf_plugin::error` paths stay valid.

pub use wf_plugin_sdk::error::{PluginError, PluginResult};
