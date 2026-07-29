pub mod context;
pub mod contributions;
pub mod dependency;
pub mod engine;
pub mod error;
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
pub use contributions::{ContributionBridge, ContributionManager, ContributionRegistrar, OverridePolicy, RegistrarGuard, PluginNodeHandler, PluginToolExecutor, PluginLLMFormatter, PluginEventHandler, PluginHookHandler, PluginMiddlewareHandler};
pub use dependency::{resolve_dependencies, ResolvedGraph};
pub use engine::{PluginEngine, PluginSystemConfig};
pub use error::{PluginError, PluginResult};
pub use events::PluginEvent;
pub use guard::PluginGuard;
pub use manifest::PluginManifest;
pub use plugin::Plugin;
pub use registry::{PluginInfo, PluginRegistry, PluginStatus};
