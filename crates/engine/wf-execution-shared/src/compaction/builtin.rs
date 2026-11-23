//! Built-in compaction engine.
//!
//! Wraps the native decision and planning functions behind the
//! [`CompactionEngine`] trait so the host drives it like any third-party
//! engine. Handoff and oldest-block dropping stay host-side safety nets and
//! are evaluated before this engine is called; this engine only owns the
//! plan-shaped compaction path.

use async_trait::async_trait;
use wf_types::message::SeqRange;

use super::engine::{
    CompactionCall, CompactionEngine, CompactionEngineResult, CompactionOutcome, EngineCapabilities,
    ManualRequest, ProgressSink, ViewPlan, BUILTIN_ENGINE_NAME,
};
use super::plan::build_plan;
use super::strategy::{compaction_strategy_name, is_single_summary_strategy};
use crate::decide_compaction;

/// Native engine backed by the built-in decision and planning pipeline.
#[derive(Debug, Default)]
pub struct BuiltinEngine;

#[async_trait]
impl CompactionEngine for BuiltinEngine {
    fn name(&self) -> &str {
        BUILTIN_ENGINE_NAME
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities::default()
    }

    async fn compact_if_needed(
        &self,
        call: &CompactionCall<'_>,
        progress: &dyn ProgressSink,
    ) -> CompactionEngineResult<Option<CompactionOutcome>> {
        progress.report("decide", 0.0);
        let decision = decide_compaction(call.hot_tokens, call.state, call.limits);
        if !matches!(decision, crate::CompactionDecision::Compact { .. }) {
            return Ok(None);
        }
        let aggressive = matches!(
            decision,
            crate::CompactionDecision::Compact { aggressive: true }
        );
        self.plan_for(call, progress, aggressive)
    }

    async fn compact_now(
        &self,
        call: &CompactionCall<'_>,
        progress: &dyn ProgressSink,
        request: &ManualRequest,
    ) -> CompactionEngineResult<CompactionOutcome> {
        progress.report("manual", 0.0);
        let aggressive = request.force;
        self.plan_for(call, progress, aggressive)
            .map(|outcome| outcome.unwrap_or(CompactionOutcome::Noop))
    }

    async fn compact_region(
        &self,
        call: &CompactionCall<'_>,
        progress: &dyn ProgressSink,
        _range: SeqRange,
    ) -> CompactionEngineResult<CompactionOutcome> {
        progress.report("region", 0.0);
        // Overflow safety net is stateless by design: it never consults the
        // block planner or a directive. Reuse the shared stateless estimate
        // so the builtin engine stays behaviour-equivalent to the native
        // forced path; the range is advisory and covered by the snapshot.
        let items = super::plan::stateless_plan_for_estimate(
            call.messages,
            call.hot_tokens,
            call.context_limit,
            call.limits,
        );
        if items.is_empty() {
            return Ok(CompactionOutcome::Noop);
        }
        progress.report("region-plan", 1.0);
        Ok(CompactionOutcome::View(ViewPlan {
            items,
            drops: Vec::new(),
            directive: None,
            capped: false,
            aggressive: true,
            freed_tokens: 0,
        }))
    }
}

impl BuiltinEngine {
    fn plan_for(
        &self,
        call: &CompactionCall<'_>,
        progress: &dyn ProgressSink,
        aggressive: bool,
    ) -> CompactionEngineResult<Option<CompactionOutcome>> {
        let plan = build_plan(
            call.state,
            call.view,
            call.messages,
            call.history,
            call.seqs,
            call.hot_tokens,
            call.limits,
            aggressive,
        );
        let single_summary = is_single_summary_strategy(&compaction_strategy_name(call.limits));
        if plan.is_empty() && !single_summary {
            progress.report("hold", 1.0);
            return Ok(None);
        }
        progress.report("plan", 1.0);
        let freed_tokens = plan.freed_tokens;
        Ok(Some(CompactionOutcome::View(ViewPlan {
            items: plan.items,
            drops: plan.drops,
            directive: plan.directive,
            capped: plan.capped,
            aggressive,
            freed_tokens,
        })))
    }
}
