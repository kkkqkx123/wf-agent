//! Plugin-author contract layer for the wf-agent plugin system.
//!
//! This crate carries only the types a plugin author depends on: the plugin
//! manifest, contribution handler traits, the stable error type, and the C
//! ABI v1 definitions for native plugins. Host-side engine machinery
//! (discovery, activation, contribution registries) lives in `wf-plugin`,
//! which re-exports this crate so existing `wf_plugin::` paths stay valid.

pub mod config;
pub mod contributions;
pub mod error;
pub mod manifest;
pub mod native;
pub mod plugin;

pub use config::validate_config_for;
pub use contributions::{
    PluginEventHandler, PluginExecutionContext, PluginLlmConfig, PluginLlmFormatter,
    PluginLlmRequest, PluginLlmResponse, PluginLlmUsage, PluginMessage, PluginMiddlewareHandler,
    PluginNodeHandler, PluginNodeResult, PluginToolContext, PluginToolExecutor, PluginToolResult,
};
pub use error::{PluginError, PluginResult};
pub use manifest::{PluginManifest, PluginPermission, PluginType};
pub use native::{ContributionRegistrarC, DispatchFn, PluginContextC, WF_PLUGIN_ABI_VERSION};
#[doc(hidden)]
pub use plugin::__private;
pub use plugin::{NativeRegistrar, PluginState, WfNativePlugin};
