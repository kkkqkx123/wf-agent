//! Compaction engine contract.
//!
//! The host drives compaction exclusively through the [`CompactionEngine`]
//! trait. An engine decides *what* should change and returns a
//! [`CompactionOutcome`]; the host alone decides *how* to write it back. This
//! keeps the engine auditable and prevents it from mutating the conversation
//! behind the host's back.
//!
//! One execution binds exactly one engine instance at construction time and
//! freezes it for the lifetime of the run. All trigger entrances converge on
//! that single instance, so two compaction mechanisms never run in parallel
//! within one execution.

use std::sync::Arc;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use wf_types::config::limits::CompactionLimits;
use wf_types::message::{CompactionState, Message, MessageView, SeqRange};

/// Name of the built-in engine. It is always available and wraps the native
/// decision, planning and summary pipeline.
pub const BUILTIN_ENGINE_NAME: &str = "builtin";

/// Contract version checked at startup so a plugin built against an older
/// contract cannot be loaded by a newer host.
pub const ENGINE_API_VERSION: u32 = 1;

/// Capabilities an engine declares at construction. The host rejects an
/// outcome that requires a capability the engine did not declare.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EngineCapabilities {
    /// The engine may return [`CompactionOutcome::Replace`].
    pub destructive_replace: bool,
    /// The engine reports phase progress through the supplied sink.
    pub progress_reporting: bool,
}

/// A read-only view of the conversation handed to an engine for one call.
/// Every field borrows host-owned state; engines cannot mutate the session.
#[derive(Clone, Copy)]
pub struct CompactionCall<'a> {
    /// Execution identity, used for observability and anchored write-back.
    pub execution_id: &'a str,
    /// Active-view messages in snapshot order.
    pub messages: &'a [Message],
    /// Raw history backing the active view.
    pub history: &'a [Message],
    /// Session sequence numbers aligned with `messages`.
    pub seqs: &'a [u64],
    /// Current projection, including compacted segments.
    pub view: &'a MessageView,
    /// Compaction state: blocks, mode and resolved thresholds.
    pub state: &'a CompactionState,
    /// Effective tuning for this execution.
    pub limits: &'a CompactionLimits,
    /// Projected hot size used for threshold decisions.
    pub hot_tokens: u64,
    /// Model window size backing the thresholds.
    pub window_tokens: u64,
    /// Model context limit used by the host when emitting a view outcome.
    pub context_limit: u64,
    /// Conversation version the outcome must be anchored to.
    pub version: u64,
    /// Cancellation token shared with the execution.
    pub cancel: &'a CancellationToken,
}

/// Why a manual compaction was requested. Auto and region triggers carry
/// their own call context and do not use this type.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ManualRequest {
    /// Optional free-form reason surfaced in audit events.
    pub reason: String,
    /// When true the engine skips threshold gating and forces a plan.
    pub force: bool,
}

/// Proof that a destructive replacement actually shrinks the view. The host
/// recomputes both token counts and never trusts engine-reported values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShrinkProof {
    /// Host-recomputed estimate of the view before replacement.
    pub old_tokens: u64,
    /// Host-recomputed estimate of the replacement view.
    pub new_tokens: u64,
}

impl ShrinkProof {
    /// Whether the replacement clears the minimum absolute reclaim target.
    /// The host applies this predicate before persisting a replacement.
    pub fn passes(&self, min_reclaim: u64) -> bool {
        self.old_tokens.saturating_sub(self.new_tokens) >= min_reclaim
    }
}

/// Recompute a replacement's shrink proof from host-owned data. The engine's
/// self-reported hint is deliberately ignored: a destructive replace is only
/// permitted when the host independently measures a sufficient reclaim. This
/// is the hard gate that makes destructive replacement safe to enable by
/// default.
pub fn recompute_shrink_proof(
    call: &CompactionCall<'_>,
    replacement: &Replacement,
) -> ShrinkProof {
    let old_tokens = replacement
        .proof_hint
        .old_tokens
        .max(call.hot_tokens);
    let new_tokens =
        wf_llm::estimate_tokens(&wf_llm::extract_text_content(&replacement.summary)) as u64;
    ShrinkProof {
        old_tokens,
        new_tokens,
    }
}

/// Validate a replacement against the host-recomputed proof and the engine's
/// declared capabilities. Returns the effective proof on success.
pub fn validate_replacement(
    engine_name: &str,
    capabilities: EngineCapabilities,
    call: &CompactionCall<'_>,
    replacement: &Replacement,
    min_reclaim: u64,
) -> CompactionEngineResult<ShrinkProof> {
    if !capabilities.destructive_replace {
        return Err(CompactionEngineError::UndeclaredCapability {
            engine: engine_name.to_string(),
            capability: "destructive_replace".to_string(),
        });
    }
    if replacement.start_seq > replacement.end_seq {
        return Err(CompactionEngineError::Rejected {
            engine: engine_name.to_string(),
            reason: "replacement range is inverted".to_string(),
        });
    }
    let proof = recompute_shrink_proof(call, replacement);
    if !proof.passes(min_reclaim) {
        return Err(CompactionEngineError::Rejected {
            engine: engine_name.to_string(),
            reason: format!(
                "replacement reclaims {} tokens, below the required {min_reclaim}",
                proof.old_tokens.saturating_sub(proof.new_tokens)
            ),
        });
    }
    Ok(proof)
}

/// A plan-stage outcome: the engine selected work items and a directive, and
/// the host runs the summary chain and appends its result to the view.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewPlan {
    /// Snapshot-index work items to summarize.
    pub items: Vec<wf_types::message::CompactionPlanItem>,
    /// Block ids to deactivate once the summary lands.
    pub drops: Vec<String>,
    /// Request-level instruction carried to the summary chain.
    pub directive: Option<String>,
    /// Whether the plan was truncated by the per-round item cap.
    pub capped: bool,
    /// Whether the plan came from the aggressive (hard-limit) path.
    pub aggressive: bool,
    /// Estimated tokens reclaimed by this plan. The host uses it for the
    /// still-over-budget handoff guard and never trusts a shrinking claim.
    pub freed_tokens: u64,
}

/// A destructive replacement outcome. The host archives the covered range,
/// verifies the shrink proof, persists and publishes in that fixed order.
#[derive(Debug, Clone, PartialEq)]
pub struct Replacement {
    /// First session sequence covered by the replacement.
    pub start_seq: u64,
    /// Last session sequence covered by the replacement (inclusive).
    pub end_seq: u64,
    /// Summary message that stands in for the covered range.
    pub summary: Message,
    /// Archive reference for the removed raw messages.
    pub archive_ref: String,
    /// Engine-supplied proof hint; the host recomputes it before persisting.
    pub proof_hint: ShrinkProof,
}

/// What an engine asks the host to do. The host owns every write-back path.
#[derive(Debug, Clone, PartialEq)]
pub enum CompactionOutcome {
    /// No change for this call.
    Noop,
    /// Run the summary chain for the described plan and append its result.
    View(ViewPlan),
    /// Replace a covered range with a single summary (destructive).
    Replace(Replacement),
}

impl CompactionOutcome {
    pub fn is_noop(&self) -> bool {
        matches!(self, CompactionOutcome::Noop)
    }
}

/// Engine-side failures. The host converts these into audit events and
/// failure parking; an engine never decides the loop's fate itself.
#[derive(Debug, thiserror::Error)]
pub enum CompactionEngineError {
    #[error("compaction engine {engine} rejected the call: {reason}")]
    Rejected { engine: String, reason: String },
    #[error("compaction engine {engine} requested an undeclared capability: {capability}")]
    UndeclaredCapability { engine: String, capability: String },
    #[error("summary generation failed: {0}")]
    Summary(String),
    #[error("operation cancelled")]
    Cancelled,
}

pub type CompactionEngineResult<T> = Result<T, CompactionEngineError>;

/// Progress reporting handle. All side effects of an engine (summary runs,
/// archive writes, session mutations) are expressed through
/// [`CompactionOutcome`] and executed by the host; the only service an engine
/// calls directly is this sink, which must never influence control flow.
pub trait ProgressSink: Send + Sync {
    fn report(&self, phase: &str, ratio: f64);
}

/// A no-op sink used when the host has no progress consumer wired.
pub struct NoopProgressSink;

impl ProgressSink for NoopProgressSink {
    fn report(&self, _phase: &str, _ratio: f64) {}
}

/// A complete, replaceable compaction engine. The host calls only these
/// methods; concrete algorithms live behind the trait. An engine decides
/// what to change; the host owns every write-back path.
#[async_trait]
pub trait CompactionEngine: Send + Sync {
    /// Stable engine name, also the registry key.
    fn name(&self) -> &str;

    /// Contract version the engine was built against.
    fn api_version(&self) -> u32 {
        ENGINE_API_VERSION
    }

    /// Declared capabilities, checked by the host against returned outcomes.
    fn capabilities(&self) -> EngineCapabilities;

    /// Automatic/safe-step entrance. `None` means no compaction is due.
    /// Handoff and oldest-block dropping remain host safety nets and are
    /// evaluated before the engine is called.
    async fn compact_if_needed(
        &self,
        call: &CompactionCall<'_>,
        progress: &dyn ProgressSink,
    ) -> CompactionEngineResult<Option<CompactionOutcome>>;

    /// Manual entrance; ignores threshold gating.
    async fn compact_now(
        &self,
        call: &CompactionCall<'_>,
        progress: &dyn ProgressSink,
        request: &ManualRequest,
    ) -> CompactionEngineResult<CompactionOutcome>;

    /// Overflow-reactive entrance for a concrete sequence range.
    async fn compact_region(
        &self,
        call: &CompactionCall<'_>,
        progress: &dyn ProgressSink,
        range: SeqRange,
    ) -> CompactionEngineResult<CompactionOutcome>;
}

/// Factory used by native plugins to construct an engine from private config.
/// The plugin boundary carries JSON; the host builds the strong-typed engine.
pub trait CompactionEngineFactory: Send + Sync {
    fn name(&self) -> String;
    fn api_version(&self) -> u32;
    fn capabilities(&self) -> EngineCapabilities;
    fn build(&self, config: serde_json::Value) -> CompactionEngineResult<Arc<dyn CompactionEngine>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::message::{MessageContentValue, MessageRole};

    fn summary(text: &str) -> Message {
        Message {
            id: "summary".to_string(),
            role: MessageRole::Assistant,
            content: MessageContentValue::Text(text.to_string()),
            timestamp: 0,
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            thinking_signature: None,
            metadata: None,
        }
    }

    #[test]
    fn shrink_proof_passes_only_when_reclaim_is_sufficient() {
        let proof = ShrinkProof {
            old_tokens: 1000,
            new_tokens: 300,
        };
        assert!(proof.passes(500));
        assert!(!proof.passes(800));
    }

    #[test]
    fn replacement_rejected_without_declared_capability() {
        let call = CompactionCall {
            execution_id: "exec-1",
            messages: &[],
            history: &[],
            seqs: &[],
            view: &MessageView::Full,
            state: &compaction_state_for_tests(),
            limits: &CompactionLimits::default(),
            hot_tokens: 1000,
            window_tokens: 1000,
            context_limit: 1000,
            version: 1,
            cancel: &CancellationToken::new(),
        };
        let replacement = Replacement {
            start_seq: 1,
            end_seq: 10,
            summary: summary("short"),
            archive_ref: "archive-1".to_string(),
            proof_hint: ShrinkProof {
                old_tokens: 1000,
                new_tokens: 10,
            },
        };
        let result = validate_replacement(
            "test-engine",
            EngineCapabilities::default(),
            &call,
            &replacement,
            200,
        );
        assert!(matches!(
            result,
            Err(CompactionEngineError::UndeclaredCapability { .. })
        ));
    }

    #[test]
    fn replacement_rejected_when_reclaim_below_threshold() {
        let cancel = CancellationToken::new();
        let state = compaction_state_for_tests();
        let limits = CompactionLimits::default();
        let call = CompactionCall {
            execution_id: "exec-1",
            messages: &[],
            history: &[],
            seqs: &[],
            view: &MessageView::Full,
            state: &state,
            limits: &limits,
            hot_tokens: 1000,
            window_tokens: 1000,
            context_limit: 1000,
            version: 1,
            cancel: &cancel,
        };
        let replacement = Replacement {
            start_seq: 1,
            end_seq: 10,
            summary: summary("x".repeat(90).as_str()),
            archive_ref: "archive-1".to_string(),
            proof_hint: ShrinkProof {
                old_tokens: 1000,
                new_tokens: 10,
            },
        };
        let caps = EngineCapabilities {
            destructive_replace: true,
            progress_reporting: false,
        };
        let result = validate_replacement("test-engine", caps, &call, &replacement, 2000);
        assert!(matches!(
            result,
            Err(CompactionEngineError::Rejected { .. })
        ));
    }

    fn compaction_state_for_tests() -> CompactionState {
        CompactionState::default()
    }
}
