//! Trusted in-process backend: native libraries share the host address
//! space and receive no sandboxing. Load only fully trusted first-party
//! builds; route untrusted extensions through the wasm backend instead.
//! Admission checks (path containment, ABI version, manifest identity)
//! keep misplaced files out but are not a runtime security boundary.

pub mod abi;
pub mod loader;
pub mod plugin;

pub use abi::{load_abi_info, ContributionRegistrarC, PluginAbiResult, PluginContextC};
pub use loader::load_native_plugin;
pub use plugin::NativePlugin;
