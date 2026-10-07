use wf_types::checkpoint::CheckpointContentConfig;

/// Filters snapshot content domains by the policy's content configuration.
/// The filter applies no defaults of its own: the config passed in must
/// already be materialized by `StandardStrategy::from_policy`, the single
/// definition point of the content defaults.
pub struct ContentFilter;

impl ContentFilter {
    pub fn new() -> Self {
        Self
    }

    pub fn should_include_state(&self, config: &CheckpointContentConfig) -> bool {
        config
            .include_state
            .expect("content config must be materialized by from_policy before filtering")
    }

    pub fn should_include_history(&self, config: &CheckpointContentConfig) -> bool {
        config
            .include_history
            .expect("content config must be materialized by from_policy before filtering")
    }

    pub fn should_include_statistics(&self, config: &CheckpointContentConfig) -> bool {
        config
            .include_statistics
            .expect("content config must be materialized by from_policy before filtering")
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
        assert!(filter.should_include_state(&config));
        assert!(filter.should_include_history(&config));
        assert!(!filter.should_include_statistics(&config));
    }

    #[test]
    fn content_filter_respects_false() {
        let filter = ContentFilter::new();
        let config = materialized(Some(false), Some(false), Some(false));
        assert!(!filter.should_include_state(&config));
        assert!(!filter.should_include_history(&config));
        assert!(!filter.should_include_statistics(&config));
    }
}
