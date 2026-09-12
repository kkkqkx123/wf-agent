//! Dynamic hook handlers: the behavior side of the hook pipeline.
//!
//! Hooks are declarative boundary points: the engine stops at the point and
//! synchronously notifies every handler that passes evaluation. Handlers
//! register into a [`HookHandlerRegistry`](crate::hooks::registry::HookHandlerRegistry)
//! under a stable name and are awaited by the engine; behavior always lives
//! here, never in the hook pipeline.

use async_trait::async_trait;

use crate::hooks::types::{HookContext, HookOutcome};

/// A runtime-registered hook handler.
///
/// `name` is the stable identifier used for registration dedup, unregister
/// and resolution from `HookDefinition.handler`; `on_point` is invoked
/// synchronously by the engine at the hook point and must be fast (per-call
/// timeout and cancellation are guarded by the registry).
#[async_trait]
pub trait HookHandler: Send + Sync {
    /// Stable handler name (registration dedup / unregister / resolution).
    fn name(&self) -> &str;
    /// Handle one hook notification. The returned outcome is aggregated by
    /// the firer: `Continue` always proceeds; `Veto` denies the guarded
    /// step, but only at gate points that opt into it (`BEFORE_EXECUTE` on
    /// the workflow node path, `BEFORE_TOOL_CALL` on the agent tool path).
    /// At every other point a veto is recorded on the audit event and
    /// otherwise treated as `Continue`. Handlers complete before the
    /// `HOOK_TRIGGERED` audit event is published, so a trigger template
    /// matching that event always starts after them; the engine does not
    /// wait for trigger completion.
    async fn on_point(&self, ctx: &HookContext) -> HookOutcome;
}
