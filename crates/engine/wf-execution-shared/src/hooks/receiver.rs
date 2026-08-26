//! Dynamic hook receivers: the behavior side of the hook pipeline.
//!
//! Hooks are declarative boundary points: the engine stops at the point and
//! synchronously notifies every receiver that passes evaluation. Receivers
//! register into a [`HookRegistry`](crate::hooks::registry::HookRegistry)
//! under a stable name and are awaited by the engine; behavior always lives
//! here, never in the hook pipeline.

use async_trait::async_trait;

use crate::hooks::types::{HookContext, HookOutcome};

/// A runtime-registered hook receiver.
///
/// `name` is the stable identifier used for registration dedup, unregister
/// and resolution from `BaseHookDefinition.receiver`; `on_hook` is invoked
/// synchronously by the engine at the hook point and must be fast (per-call
/// timeout and cancellation are guarded by the registry).
#[async_trait]
pub trait HookReceiver: Send + Sync {
    /// Stable receiver name (registration dedup / unregister / resolution).
    fn name(&self) -> &str;
    /// Handle one hook notification. The returned outcome is aggregated by
    /// the dispatcher; `Intercept` is reserved for future control semantics.
    async fn on_hook(&self, ctx: &HookContext) -> HookOutcome;
}
