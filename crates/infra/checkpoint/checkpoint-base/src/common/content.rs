use wf_types::checkpoint::CheckpointContentConfig;

pub struct ContentFilter;

impl ContentFilter {
    pub fn new() -> Self {
        Self
    }

    pub fn should_include_state(&self, config: &CheckpointContentConfig) -> bool {
        config.include_state.unwrap_or(true)
    }

    pub fn should_include_history(&self, config: &CheckpointContentConfig) -> bool {
        config.include_history.unwrap_or(true)
    }

    pub fn should_include_statistics(&self, config: &CheckpointContentConfig) -> bool {
        config.include_statistics.unwrap_or(true)
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

    #[test]
    fn content_filter_defaults() {
        let filter = ContentFilter::new();
        let config = CheckpointContentConfig {
            include_state: None,
            include_history: None,
            include_statistics: None,
            metadata: None,
            asynchronous: None,
        };
        assert!(filter.should_include_state(&config));
        assert!(filter.should_include_history(&config));
        assert!(filter.should_include_statistics(&config));
    }

    #[test]
    fn content_filter_respects_false() {
        let filter = ContentFilter::new();
        let config = CheckpointContentConfig {
            include_state: Some(false),
            include_history: Some(false),
            include_statistics: Some(false),
            metadata: None,
            asynchronous: None,
        };
        assert!(!filter.should_include_state(&config));
        assert!(!filter.should_include_history(&config));
        assert!(!filter.should_include_statistics(&config));
    }
}
