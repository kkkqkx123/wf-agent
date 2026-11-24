//! Minimal third-party compaction engine demo.
//!
//! This crate shows how a native engine implements the `CompactionEngine`
//! contract and registers a process-level factory that the host resolves by
//! name (`compaction.engine = "demo-view-only"`). The engine deliberately
//! keeps behaviour simple and deterministic: it never auto-triggers, and on
//! a manual/region call it returns a stateless legacy view plan. It declares
//! no destructive capability, so a `Replace` from this engine would be
//! rejected by the host shrink-proof gate — demonstrating the capability
//! boundary as well as full replaceability of `builtin`.

use std::sync::Arc;

use async_trait::async_trait;
use wf_execution_shared::{
    register_compaction_engine, unregister_compaction_engine, CompactionCall, CompactionEngine,
    CompactionEngineFactory, CompactionEngineResult, CompactionOutcome, EngineCapabilities,
    ManualRequest, ProgressSink, ViewPlan, ENGINE_API_VERSION,
};
use wf_types::message::SeqRange;

/// Engine name used in `compaction.engine` configuration.
pub const DEMO_ENGINE_NAME: &str = "demo-view-only";

/// A minimal engine that only ever produces a legacy single-summary view.
pub struct DemoViewEngine;

#[async_trait]
impl CompactionEngine for DemoViewEngine {
    fn name(&self) -> &str {
        DEMO_ENGINE_NAME
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            destructive_replace: false,
            progress_reporting: true,
        }
    }

    async fn compact_if_needed(
        &self,
        _call: &CompactionCall<'_>,
        _progress: &dyn ProgressSink,
    ) -> CompactionEngineResult<Option<CompactionOutcome>> {
        // Auto settlement stays with the host; this demo never self-triggers.
        Ok(None)
    }

    async fn compact_now(
        &self,
        call: &CompactionCall<'_>,
        progress: &dyn ProgressSink,
        _request: &ManualRequest,
    ) -> CompactionEngineResult<CompactionOutcome> {
        progress.report("plan", 0.5);
        if call.messages.is_empty() {
            return Ok(CompactionOutcome::Noop);
        }
        progress.report("done", 1.0);
        Ok(CompactionOutcome::View(ViewPlan {
            items: Vec::new(),
            drops: Vec::new(),
            directive: None,
            capped: false,
            aggressive: false,
            freed_tokens: 0,
        }))
    }

    async fn compact_region(
        &self,
        _call: &CompactionCall<'_>,
        progress: &dyn ProgressSink,
        _range: SeqRange,
    ) -> CompactionEngineResult<CompactionOutcome> {
        progress.report("region", 1.0);
        Ok(CompactionOutcome::Noop)
    }
}

/// Factory used by the host to construct the demo engine from private JSON.
pub struct DemoFactory;

impl CompactionEngineFactory for DemoFactory {
    fn name(&self) -> String {
        DEMO_ENGINE_NAME.to_string()
    }

    fn api_version(&self) -> u32 {
        ENGINE_API_VERSION
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            destructive_replace: false,
            progress_reporting: true,
        }
    }

    fn build(&self, _config: serde_json::Value) -> CompactionEngineResult<Arc<dyn CompactionEngine>> {
        Ok(Arc::new(DemoViewEngine))
    }
}

/// Register the demo engine factory into the host process registry. Call
/// once during plugin/native host startup before executions are assembled.
/// Returns `Err` when the name is taken or the contract version mismatches.
pub fn register() -> Result<(), String> {
    register_compaction_engine(Arc::new(DemoFactory))
}

/// Unregister the demo engine factory (host shutdown / tests).
pub fn unregister() -> bool {
    unregister_compaction_engine(DEMO_ENGINE_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_execution_shared::build_compaction_engine;

    #[test]
    fn demo_engine_builds_from_registry_and_replaces_builtin() {
        // The registry is process-global; clear any prior registration so
        // repeated test runs stay deterministic.
        unregister();
        assert!(register().is_ok(), "factory registers once");
        let engine = build_compaction_engine(DEMO_ENGINE_NAME).expect("demo engine builds");
        assert_eq!(engine.name(), DEMO_ENGINE_NAME);
        assert!(!engine.capabilities().destructive_replace);
        // Re-registration is rejected so no two factories share a name.
        assert!(register().is_err());
        assert!(unregister());
    }
}
