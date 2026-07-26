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

pub struct SizeBudget {
    max_snapshot_bytes: usize,
    max_message_count: usize,
}

impl SizeBudget {
    pub fn new(max_snapshot_bytes: usize, max_message_count: usize) -> Self {
        Self {
            max_snapshot_bytes,
            max_message_count,
        }
    }

    pub fn default_budget() -> Self {
        Self::new(1024 * 1024, 100)
    }

    pub fn truncate_messages<T: Clone>(&self, messages: Option<Vec<T>>) -> Option<Vec<T>> {
        messages.map(|msgs| {
            if msgs.len() > self.max_message_count {
                msgs[msgs.len() - self.max_message_count..].to_vec()
            } else {
                msgs
            }
        })
    }

    pub fn is_within_budget(&self, snapshot_bytes: usize) -> bool {
        snapshot_bytes <= self.max_snapshot_bytes
    }

    pub fn max_snapshot_bytes(&self) -> usize {
        self.max_snapshot_bytes
    }

    pub fn max_message_count(&self) -> usize {
        self.max_message_count
    }
}

impl Default for SizeBudget {
    fn default() -> Self {
        Self::default_budget()
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
        };
        assert!(!filter.should_include_state(&config));
        assert!(!filter.should_include_history(&config));
        assert!(!filter.should_include_statistics(&config));
    }

    #[test]
    fn size_budget_truncates_messages() {
        let budget = SizeBudget::new(1024, 3);
        let messages: Vec<i32> = vec![1, 2, 3, 4, 5];
        let truncated = budget.truncate_messages(Some(messages)).unwrap();
        assert_eq!(truncated, vec![3, 4, 5]);
    }

    #[test]
    fn size_budget_keeps_small_vectors() {
        let budget = SizeBudget::new(1024, 10);
        let messages = vec![1, 2, 3];
        let result = budget.truncate_messages(Some(messages)).unwrap();
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn size_budget_checks_bytes() {
        let budget = SizeBudget::new(100, 10);
        assert!(budget.is_within_budget(50));
        assert!(budget.is_within_budget(100));
        assert!(!budget.is_within_budget(101));
    }
}
