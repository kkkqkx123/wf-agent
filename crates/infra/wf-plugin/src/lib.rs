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
pub use manifest::{PluginManifest, PluginPermission, PluginType};
pub use package::{InstalledPlugin, PackageState, PluginPackageManager};
pub use plugin::Plugin;
pub use registry::{ContributionRecord, PluginInfo, PluginRegistry, PluginStatus};
