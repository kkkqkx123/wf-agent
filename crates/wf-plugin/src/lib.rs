pub mod context;
pub mod contributions;
pub mod dependency;
pub mod engine;
pub mod error;
pub mod event_bus;
pub mod events;
pub mod guard;
pub mod manifest;
pub mod plugin;
pub mod registry;

#[cfg(feature = "lua")]
pub mod lua;

#[cfg(feature = "native")]
pub mod native;

pub use context::PluginContext;
pub use contributions::{
    middleware_phase, ContributionBridge, ContributionManager, ContributionRegistrar, NextFn,
    OverridePolicy, PluginEventHandler, PluginHookHandler, PluginLLMFormatter,
    PluginMiddlewareHandler, PluginNodeHandler, PluginToolExecutor, RegistrarGuard,
};
pub use dependency::{resolve_dependencies, ResolvedGraph};
pub use engine::{PluginEngine, PluginSystemConfig};
pub use error::{PluginError, PluginResult};
pub use event_bus::{PluginEventBus, PluginEventSubscription};
pub use events::PluginEvent;
pub use guard::PluginGuard;
pub use manifest::{PluginManifest, PluginType};
pub use plugin::Plugin;
pub use registry::{
    ContributionRecord, DiscoveredPlugin, PluginInfo, PluginRegistry, PluginStatus,
};
