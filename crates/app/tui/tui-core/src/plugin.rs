//! Plugin host and UI slot registry.
//!
//! A plugin contributes UI by registering slot content while it activates; the
//! host owns every plugin and the content they registered, so a later
//! deactivation removes exactly what the plugin added. This mirrors the modal
//! stack: contributions are heterogeneous and appear/disappear at runtime, so
//! the host stores them behind `Box<dyn ...>` while every other layer stays
//! concrete.
//!
//! Registration is compile-time (the shell registers built-in plugins at
//! startup) and activation is runtime, so a plugin can be disabled without
//! rebuilding. The host never re-reads plugin code after activation: it works
//! off the recorded contributions.

pub mod api;
pub mod host;
pub mod slot;

pub use api::PluginApi;
pub use host::{PluginHandle, PluginHost};
pub use slot::{FnSlot, Slot, SlotContent, SlotContext};

/// A UI extension unit, parameterized by the per-frame slot payload `D`.
///
/// [`Plugin::activate`] registers contributions through [`PluginApi`]; the host
/// records them under the plugin id. [`Plugin::deactivate`] releases state the
/// plugin owns directly; host-owned contributions are removed by the host
/// itself.
pub trait Plugin<D>: Send + Sync + 'static {
    /// Stable identity used for registration and removal.
    fn id(&self) -> &str;

    /// Register this plugin's contributions.
    fn activate(&mut self, api: &mut PluginApi<D>);

    /// Release state the plugin owns directly.
    fn deactivate(&mut self);
}
