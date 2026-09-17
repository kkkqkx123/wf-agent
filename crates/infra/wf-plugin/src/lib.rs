//! Plugin runtime: one trait, one manifest, one contribution model,
//! three backends with tiered isolation.
//!
//! Every backend implements [`Plugin`] and registers the same contribution
//! kinds through `ContributionRegistrar`, so the engine never observes
//! which backend a plugin uses. Isolation differs by construction: wasm
//! guests run under fuel, epoch, memory, and WASI-grant enforcement;
//! lua scripts run behind a restricted standard library plus interpreter
//! hooks; native libraries run in-process with no sandbox and are reserved
//! for fully trusted first-party builds.
//!
//! Limits resolve weakest to strongest as built-in defaults, engine
//! globals (`PluginSystemConfig`), then the per-plugin manifest. Declared
//! permissions are runtime-enforced only for wasm; elsewhere they are
//! admission and audit input.

pub mod context;
pub mod contributions;
pub mod dependency;
pub mod engine;
pub mod error;
pub mod event_bus;
pub mod events;
pub mod guard;
pub mod manifest;
pub mod package;
pub mod plugin;
pub mod registry;
pub mod signing;

#[cfg(feature = "lua")]
pub mod lua;

#[cfg(feature = "native")]
pub mod native;

#[cfg(feature = "wasm")]
pub mod wasm;

pub use context::PluginContext;
pub use contributions::{
    parse_middleware_outcome, CodecHttpRequest, ContributionBridge, ContributionManager,
    ContributionRegistrar, ContributionType, MiddlewareOutcome, NextFn, OverridePolicy,
    PluginEventData, PluginEventHandler, PluginExecutionContext, PluginLlmCodec,
    PluginMiddlewareHandler, PluginNodeHandler, PluginNodeResult, PluginToolContext,
    PluginToolExecutor, PluginToolResult, RegistrarGuard,
};
pub use dependency::{resolve_dependencies, ResolvedGraph};
pub use engine::{PluginEngine, PluginSystemConfig};
pub use error::{PluginError, PluginResult};
pub use event_bus::{PluginEventBus, PluginEventSubscription};
pub use events::PluginEvent;
pub use guard::PluginGuard;
pub use manifest::{LuaConfig, PluginManifest, PluginPermission, PluginType, WasmConfig};
pub use package::{InstalledPlugin, PackageState, PluginPackageManager};
pub use plugin::Plugin;
pub use registry::{ContributionRecord, PluginInfo, PluginRegistry, PluginStatus};
