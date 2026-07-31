use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct DeadLoopDetectionConfig {
    pub enabled: bool,
    pub short_sequence_window: usize,
    pub min_repeat_unit_length: usize,
    pub min_repeat_count: usize,
    pub max_period_length: usize,
}

impl Default for DeadLoopDetectionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            short_sequence_window: 100,
            min_repeat_unit_length: 10,
            min_repeat_count: 3,
            max_period_length: 50,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum LoopDetectionResult {
    NoLoop,
    Detected { pattern: String, count: usize },
}

pub struct DeadLoopDetector {
    config: DeadLoopDetectionConfig,
    recent_text: String,
    tool_call_history: Vec<String>,
}

impl DeadLoopDetector {
    pub fn new(config: DeadLoopDetectionConfig) -> Self {
        Self {
            config,
            recent_text: String::new(),
            tool_call_history: Vec::new(),
        }
    }

    pub fn check_text(&mut self, text: &str) -> LoopDetectionResult {
        if !self.config.enabled {
            return LoopDetectionResult::NoLoop;
        }

        self.recent_text.push_str(text);

        if self.recent_text.len() > self.config.short_sequence_window * 2 {
            let start = self.recent_text.len() - self.config.short_sequence_window;
            self.recent_text = self.recent_text[start..].to_string();
        }

        self.detect_repetition()
    }

    pub fn check_tool_call(&mut self, tool_name: &str, arguments: &str) -> LoopDetectionResult {
        if !self.config.enabled {
            return LoopDetectionResult::NoLoop;
        }

        let call_signature = format!("{}:{}", tool_name, arguments);
        self.tool_call_history.push(call_signature.clone());

        if self.tool_call_history.len() > 20 {
            self.tool_call_history.remove(0);
        }

        let mut counts: HashMap<String, usize> = HashMap::new();
        for call in &self.tool_call_history {
            *counts.entry(call.clone()).or_insert(0) += 1;
        }

        for (call, count) in counts {
            if count >= self.config.min_repeat_count {
                return LoopDetectionResult::Detected {
                    pattern: call,
                    count,
                };
            }
        }

        LoopDetectionResult::NoLoop
    }

    fn detect_repetition(&self) -> LoopDetectionResult {
        let text = &self.recent_text;
        let len = text.len();

        if len < self.config.min_repeat_unit_length * self.config.min_repeat_count {
            return LoopDetectionResult::NoLoop;
        }

        for period in 1..=self
            .config
            .max_period_length
            .min(len / self.config.min_repeat_count)
        {
            let pattern = &text[len - period..];
            let mut count = 0;
            let mut pos = len;

            while pos >= period {
                if text[pos - period..pos] == *pattern {
                    count += 1;
                    pos -= period;
                } else {
                    break;
                }
            }

            if count >= self.config.min_repeat_count && period >= self.config.min_repeat_unit_length
            {
                return LoopDetectionResult::Detected {
                    pattern: pattern.to_string(),
                    count,
                };
            }
        }

        LoopDetectionResult::NoLoop
    }

    pub fn reset(&mut self) {
        self.recent_text.clear();
        self.tool_call_history.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_loop_in_normal_text() {
        let config = DeadLoopDetectionConfig::default();
        let mut detector = DeadLoopDetector::new(config);
        let result = detector.check_text("This is normal text with no repetition.");
        assert_eq!(result, LoopDetectionResult::NoLoop);
    }

    #[test]
    fn test_detect_repetition() {
        let config = DeadLoopDetectionConfig {
            min_repeat_unit_length: 3,
            min_repeat_count: 3,
            max_period_length: 10,
            ..Default::default()
        };
        let mut detector = DeadLoopDetector::new(config);
        let result = detector.check_text("abcabcabc");
        assert!(matches!(result, LoopDetectionResult::Detected { .. }));
    }

    #[test]
    fn test_detect_tool_call_loop() {
        let config = DeadLoopDetectionConfig {
            min_repeat_count: 3,
            ..Default::default()
        };
        let mut detector = DeadLoopDetector::new(config);

        assert_eq!(
            detector.check_tool_call("search", r#"{"q":"a"}"#),
            LoopDetectionResult::NoLoop
        );
        assert_eq!(
            detector.check_tool_call("search", r#"{"q":"a"}"#),
            LoopDetectionResult::NoLoop
        );
        let result = detector.check_tool_call("search", r#"{"q":"a"}"#);
        assert!(matches!(result, LoopDetectionResult::Detected { .. }));
    }

    #[test]
    fn test_disabled_detector() {
        let config = DeadLoopDetectionConfig {
            enabled: false,
            ..Default::default()
        };
        let mut detector = DeadLoopDetector::new(config);
        let result = detector.check_text("abcabcabc");
        assert_eq!(result, LoopDetectionResult::NoLoop);
    }
}
