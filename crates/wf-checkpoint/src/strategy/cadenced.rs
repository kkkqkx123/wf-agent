use std::collections::HashMap;

use wf_types::checkpoint::{
    CheckpointContentConfig, CheckpointContext, CheckpointRetentionConfig, CheckpointTrigger,
    UnifiedCheckpointPolicy,
};

use super::inner::{CheckpointStrategy, StandardStrategy};

/// A timing variant that maps to a canonical `CheckpointTrigger`.
pub trait CheckpointTiming: PartialEq + Eq + Clone + std::hash::Hash + std::fmt::Debug {
    fn to_trigger(&self) -> CheckpointTrigger;
}

/// `CheckpointTrigger` is itself a timing variant: cadenced strategies can be
/// driven directly by triggers (identity mapping), which is how the agent
/// coordinator's interval semantics plug into the cadenced strategy.
impl CheckpointTiming for CheckpointTrigger {
    fn to_trigger(&self) -> CheckpointTrigger {
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
pub struct CadencedCheckpointStrategy<T: CheckpointTiming> {
    inner: StandardStrategy,
    timings: Vec<T>,
    /// Per-timing cadence. Timings absent from the map pass step 3 always.
    cadences: HashMap<T, u32>,
}

impl<T: CheckpointTiming> CadencedCheckpointStrategy<T> {
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
        F: Fn(&CheckpointTrigger) -> Option<T>,
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

    impl CheckpointTiming for TestTiming {
        fn to_trigger(&self) -> CheckpointTrigger {
            match self {
                TestTiming::Before => CheckpointTrigger::BeforeExecute,
                TestTiming::After => CheckpointTrigger::AfterExecute,
                TestTiming::OnError => CheckpointTrigger::OnError,
                TestTiming::Start => CheckpointTrigger::Manual,
                TestTiming::End => CheckpointTrigger::OnComplete,
            }
        }
    }

    fn map_trigger(t: &CheckpointTrigger) -> Option<TestTiming> {
        match t {
            CheckpointTrigger::BeforeExecute => Some(TestTiming::Before),
            CheckpointTrigger::AfterExecute => Some(TestTiming::After),
            CheckpointTrigger::OnError => Some(TestTiming::OnError),
            CheckpointTrigger::Manual => Some(TestTiming::Start),
            CheckpointTrigger::OnComplete => Some(TestTiming::End),
            _ => None,
        }
    }

    fn make_policy(triggers: Vec<CheckpointTrigger>) -> UnifiedCheckpointPolicy {
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
            &make_policy(vec![CheckpointTrigger::AfterExecute]),
            map_trigger,
        );
        assert!(s.should_checkpoint(&TestTiming::After, "test", "", 1));
        assert!(!s.should_checkpoint(&TestTiming::Before, "test", "", 1));
        assert!(!s.should_checkpoint(&TestTiming::OnError, "test", "", 1));
    }

    #[test]
    fn cadence_modulo() {
        let s = CadencedCheckpointStrategy::from_policy(
            &make_policy(vec![CheckpointTrigger::AfterExecute]),
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
            &make_policy(vec![CheckpointTrigger::OnError]),
            map_trigger,
        );
        assert!(!s.should_checkpoint(&TestTiming::After, "test", "", 1));
        assert!(s.should_checkpoint(&TestTiming::OnError, "test", "", 1));
    }

    #[test]
    fn multiple_triggers() {
        let s = CadencedCheckpointStrategy::from_policy(
            &make_policy(vec![
                CheckpointTrigger::Manual,
                CheckpointTrigger::OnComplete,
            ]),
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
                CheckpointTrigger::BeforeExecute,
                CheckpointTrigger::AfterExecute,
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
                CheckpointTrigger::BeforeExecute,
                CheckpointTrigger::OnError,
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
