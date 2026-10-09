use crate::error::CheckpointError;
use wf_types::checkpoint::CheckpointContentConfig;

/// Filters snapshot content domains by the policy's content configuration.
/// The config passed in must already be materialized by
/// `StandardStrategy::from_policy`, the single definition point of the
/// content defaults. Missing domains are an explicit error, never a panic,
/// so unmaterialized configs fail loudly at the policy boundary.
pub struct ContentFilter;

impl ContentFilter {
    pub fn new() -> Self {
        Self
    }

    pub fn should_include_state(
        &self,
        config: &CheckpointContentConfig,
    ) -> Result<bool, CheckpointError> {
        config.include_state.ok_or_else(|| CheckpointError::Validation {
            reason: "content config missing include_state; materialize via from_policy before filtering".to_string(),
        })
    }

    pub fn should_include_history(
        &self,
        config: &CheckpointContentConfig,
    ) -> Result<bool, CheckpointError> {
        config.include_history.ok_or_else(|| CheckpointError::Validation {
            reason: "content config missing include_history; materialize via from_policy before filtering".to_string(),
        })
    }

    pub fn should_include_statistics(
        &self,
        config: &CheckpointContentConfig,
    ) -> Result<bool, CheckpointError> {
        config.include_statistics.ok_or_else(|| CheckpointError::Validation {
            reason: "content config missing include_statistics; materialize via from_policy before filtering".to_string(),
        })
    }
}

impl Default for ContentFilter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn materialized(
        include_state: Option<bool>,
        include_history: Option<bool>,
        include_statistics: Option<bool>,
    ) -> CheckpointContentConfig {
        CheckpointContentConfig {
            include_state,
            include_history,
            include_statistics,
            metadata: None,
            asynchronous: None,
        }
    }

    #[test]
    fn content_filter_uses_materialized_defaults() {
        let filter = ContentFilter::new();
        // The values `from_policy` materializes for an absent content config.
        let config = materialized(Some(true), Some(true), Some(false));
        assert!(filter.should_include_state(&config).unwrap());
        assert!(filter.should_include_history(&config).unwrap());
        assert!(!filter.should_include_statistics(&config).unwrap());
    }

    #[test]
    fn content_filter_respects_false() {
        let filter = ContentFilter::new();
        let config = materialized(Some(false), Some(false), Some(false));
        assert!(!filter.should_include_state(&config).unwrap());
        assert!(!filter.should_include_history(&config).unwrap());
        assert!(!filter.should_include_statistics(&config).unwrap());
    }

    #[test]
    fn content_filter_rejects_unmaterialized() {
        let filter = ContentFilter::new();
        let config = materialized(None, None, None);
        assert!(filter.should_include_state(&config).is_err());
        assert!(filter.should_include_history(&config).is_err());
        assert!(filter.should_include_statistics(&config).is_err());
    }
}
