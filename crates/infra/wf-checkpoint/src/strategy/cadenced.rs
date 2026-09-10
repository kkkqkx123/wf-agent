use std::collections::HashMap;

use wf_types::checkpoint::{
    CheckpointContentConfig, CheckpointContext, CheckpointRetentionConfig, CheckpointTiming,
    UnifiedCheckpointPolicy,
};

use super::inner::{CheckpointStrategy, StandardStrategy};

/// A timing variant that maps to a canonical `CheckpointTiming`.
pub trait CheckpointTimingVariant:
    PartialEq + Eq + Clone + std::hash::Hash + std::fmt::Debug
{
    fn to_trigger(&self) -> CheckpointTiming;
}

/// `CheckpointTiming` is itself a timing variant: cadenced strategies can be
/// driven directly by triggers (identity mapping), which is how the agent
/// coordinator's interval semantics plug into the cadenced strategy.
impl CheckpointTimingVariant for CheckpointTiming {
    fn to_trigger(&self) -> CheckpointTiming {
        self.clone()
    }
}

/// Cadenced wrapper around `StandardStrategy` that adds timing filtering
/// and an optional cadence (every N count) per timing variant.
///
/// The `should_checkpoint` decision is a three-stage pipeline:
/// 1. Delegate to `StandardStrategy` (enabled + trigger check)
/// 2. Check if the requested timing is in the enabled set
/// 3. For timings with cadence > 1, apply modulo on count
#[derive(Debug, Clone)]
pub struct CadencedCheckpointStrategy<T: CheckpointTimingVariant> {
    inner: StandardStrategy,
    timings: Vec<T>,
    /// Per-timing cadence. Timings absent from the map pass step 3 always.
    cadences: HashMap<T, u32>,
}

impl<T: CheckpointTimingVariant> CadencedCheckpointStrategy<T> {
    /// Create a fully disabled strategy that never checkpoints.
    pub fn disabled() -> Self {
        Self {
            inner: StandardStrategy::disabled(),
            timings: vec![],
            cadences: HashMap::new(),
        }
    }

    /// Build from a `UnifiedCheckpointPolicy`, mapping triggers to timing
    /// variants via the provided closure.
    pub fn from_policy<F>(policy: &UnifiedCheckpointPolicy, map_trigger: F) -> Self
    where
        F: Fn(&CheckpointTiming) -> Option<T>,
    {
        let timings: Vec<T> = policy.triggers.iter().filter_map(map_trigger).collect();
        Self {
            inner: StandardStrategy::from_policy(policy),
            timings,
            cadences: HashMap::new(),
        }
    }

    /// Set a cadence for a specific timing variant.
    /// When set, `should_checkpoint` for that timing only returns true
    /// every `n` counts. May be called multiple times for different timings.
    pub fn with_cadence(mut self, timing: T, n: u32) -> Self {
        self.cadences.insert(timing, n.max(1));
        self
    }

    /// Whether the timing belongs to the enabled set, ignoring cadence.
    /// Used by callers that need the "is this timing on at all" decision
    /// (e.g. config resolution) without committing to a specific count.
    pub fn timing_enabled(&self, timing: &T) -> bool {
        if !self.inner.should_checkpoint(
            &timing.to_trigger(),
            &CheckpointContext {
                entity_type: String::new(),
                entity_id: String::new(),
                trigger: Some(timing.to_trigger()),
                actor_id: None,
                attempt: None,
                retry_count: None,
                error: None,
                fallback_used: None,
                metadata: None,
            },
        ) {
            return false;
        }
        self.timings.contains(timing)
    }

    /// The configured cadence for the timing (defaults to 1 when unset).
    pub fn cadence(&self, timing: &T) -> u32 {
        self.cadences.get(timing).copied().unwrap_or(1)
    }

    /// Returns true if a checkpoint should fire for the given timing.
    pub fn should_checkpoint(
        &self,
        timing: &T,
        entity_type: &str,
        entity_id: &str,
        count: u32,
    ) -> bool {
        if !self.inner.should_checkpoint(
            &timing.to_trigger(),
            &CheckpointContext {
                entity_type: entity_type.to_string(),
                entity_id: entity_id.to_string(),
                trigger: Some(timing.to_trigger()),
                actor_id: None,
                attempt: None,
                retry_count: None,
                error: None,
                fallback_used: None,
                metadata: None,
            },
        ) {
            return false;
        }
        if !self.timings.contains(timing) {
            return false;
        }
        if let Some(cadence) = self.cadences.get(timing) {
            if *cadence > 1 {
                return count.is_multiple_of(*cadence);
            }
        }
        true
    }

    pub fn content_config(&self) -> &CheckpointContentConfig {
        self.inner.content_config()
    }

    pub fn retention_config(&self) -> Option<&CheckpointRetentionConfig> {
        self.inner.retention_config()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    enum TestTiming {
        Before,
        After,
        OnError,
        Start,
        End,
    }

    impl CheckpointTimingVariant for TestTiming {
        fn to_trigger(&self) -> CheckpointTiming {
            match self {
                TestTiming::Before => CheckpointTiming::BeforeExecute,
                TestTiming::After => CheckpointTiming::AfterExecute,
                TestTiming::OnError => CheckpointTiming::OnError,
                TestTiming::Start => CheckpointTiming::Manual,
                TestTiming::End => CheckpointTiming::OnComplete,
            }
        }
    }

    fn map_trigger(t: &CheckpointTiming) -> Option<TestTiming> {
        match t {
            CheckpointTiming::BeforeExecute => Some(TestTiming::Before),
            CheckpointTiming::AfterExecute => Some(TestTiming::After),
            CheckpointTiming::OnError => Some(TestTiming::OnError),
            CheckpointTiming::Manual => Some(TestTiming::Start),
            CheckpointTiming::OnComplete => Some(TestTiming::End),
            _ => None,
        }
    }

    fn make_policy(triggers: Vec<CheckpointTiming>) -> UnifiedCheckpointPolicy {
        UnifiedCheckpointPolicy {
            enabled: true,
            triggers,
            content: None,
            retention: None,
            error_handling: None,
        }
    }

    #[test]
    fn disabled_never_checkpoints() {
        let s: CadencedCheckpointStrategy<TestTiming> = CadencedCheckpointStrategy::disabled();
        assert!(!s.should_checkpoint(&TestTiming::After, "test", "", 1));
        assert!(!s.should_checkpoint(&TestTiming::OnError, "test", "", 1));
    }

    #[test]
    fn fires_for_matching_timing() {
        let s = CadencedCheckpointStrategy::from_policy(
            &make_policy(vec![CheckpointTiming::AfterExecute]),
            map_trigger,
        );
        assert!(s.should_checkpoint(&TestTiming::After, "test", "", 1));
        assert!(!s.should_checkpoint(&TestTiming::Before, "test", "", 1));
        assert!(!s.should_checkpoint(&TestTiming::OnError, "test", "", 1));
    }

    #[test]
    fn cadence_modulo() {
        let s = CadencedCheckpointStrategy::from_policy(
            &make_policy(vec![CheckpointTiming::AfterExecute]),
            map_trigger,
        )
        .with_cadence(TestTiming::After, 3);

        assert!(!s.should_checkpoint(&TestTiming::After, "test", "", 1));
        assert!(!s.should_checkpoint(&TestTiming::After, "test", "", 2));
        assert!(s.should_checkpoint(&TestTiming::After, "test", "", 3));
        assert!(!s.should_checkpoint(&TestTiming::After, "test", "", 4));
        assert!(s.should_checkpoint(&TestTiming::After, "test", "", 6));
    }

    #[test]
    fn on_error_only() {
        let s = CadencedCheckpointStrategy::from_policy(
            &make_policy(vec![CheckpointTiming::OnError]),
            map_trigger,
        );
        assert!(!s.should_checkpoint(&TestTiming::After, "test", "", 1));
        assert!(s.should_checkpoint(&TestTiming::OnError, "test", "", 1));
    }

    #[test]
    fn multiple_triggers() {
        let s = CadencedCheckpointStrategy::from_policy(
            &make_policy(vec![CheckpointTiming::Manual, CheckpointTiming::OnComplete]),
            map_trigger,
        );
        assert!(s.should_checkpoint(&TestTiming::Start, "test", "", 0));
        assert!(s.should_checkpoint(&TestTiming::End, "test", "", 0));
        assert!(!s.should_checkpoint(&TestTiming::After, "test", "", 1));
    }

    #[test]
    fn multiple_cadences_per_timing() {
        let s = CadencedCheckpointStrategy::from_policy(
            &make_policy(vec![
                CheckpointTiming::BeforeExecute,
                CheckpointTiming::AfterExecute,
            ]),
            map_trigger,
        )
        .with_cadence(TestTiming::Before, 2)
        .with_cadence(TestTiming::After, 2);

        assert!(!s.should_checkpoint(&TestTiming::Before, "test", "", 1));
        assert!(s.should_checkpoint(&TestTiming::Before, "test", "", 2));
        assert!(!s.should_checkpoint(&TestTiming::Before, "test", "", 3));
        assert!(s.should_checkpoint(&TestTiming::Before, "test", "", 4));

        assert!(!s.should_checkpoint(&TestTiming::After, "test", "", 1));
        assert!(s.should_checkpoint(&TestTiming::After, "test", "", 2));
        assert!(!s.should_checkpoint(&TestTiming::After, "test", "", 3));
    }

    #[test]
    fn mixed_cadence_and_uncadenced_timings() {
        let s = CadencedCheckpointStrategy::from_policy(
            &make_policy(vec![
                CheckpointTiming::BeforeExecute,
                CheckpointTiming::OnError,
            ]),
            map_trigger,
        )
        .with_cadence(TestTiming::Before, 3);

        // Cadenced timing fires every 3 counts.
        assert!(!s.should_checkpoint(&TestTiming::Before, "test", "", 1));
        assert!(s.should_checkpoint(&TestTiming::Before, "test", "", 3));
        // Uncadenced timing fires every occurrence.
        assert!(s.should_checkpoint(&TestTiming::OnError, "test", "", 1));
        assert!(s.should_checkpoint(&TestTiming::OnError, "test", "", 7));
    }
}
