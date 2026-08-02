use wf_types::llm::DeadLoopDetectionConfig;

/// Result of dead loop detection.
#[derive(Debug, Clone)]
pub struct DeadLoopDetectionResult {
    pub detected: bool,
    pub loop_type: Option<LoopType>,
    pub details: Option<String>,
}

#[derive(Debug, Clone)]
pub enum LoopType {
    ShortSequence,
    ParagraphRepeat,
    ListRepeat,
}

/// Dead loop detector for reasoning content.
pub struct DeadLoopDetector {
    config: DeadLoopDetectorConfig,
    checked_checkpoints: std::collections::HashSet<u32>,
}

#[derive(Debug, Clone)]
pub struct DeadLoopDetectorConfig {
    pub enabled: bool,
    pub checkpoints: Vec<u32>,
    pub short_sequence_window: u32,
    pub min_repeat_unit_length: u32,
    pub min_repeat_count: u32,
    pub min_period_elements: u32,
    pub max_period_length: u32,
}

impl Default for DeadLoopDetectorConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            checkpoints: vec![500, 1000, 2000],
            short_sequence_window: 200,
            min_repeat_unit_length: 2,
            min_repeat_count: 4,
            min_period_elements: 6,
            max_period_length: 50,
        }
    }
}

impl From<&DeadLoopDetectionConfig> for DeadLoopDetectorConfig {
    fn from(config: &DeadLoopDetectionConfig) -> Self {
        Self {
            enabled: config.enabled.unwrap_or(true),
            checkpoints: config.checkpoints.clone().unwrap_or(vec![500, 1000, 2000]),
            short_sequence_window: config.short_sequence_window.unwrap_or(200),
            min_repeat_unit_length: config.min_repeat_unit_length.unwrap_or(2),
            min_repeat_count: config.min_repeat_count.unwrap_or(4),
            min_period_elements: config.min_period_elements.unwrap_or(6),
            max_period_length: config.max_period_length.unwrap_or(50),
        }
    }
}

impl Default for DeadLoopDetector {
    fn default() -> Self {
        Self::new(DeadLoopDetectorConfig::default())
    }
}

impl DeadLoopDetector {
    pub fn new(config: DeadLoopDetectorConfig) -> Self {
        // Checkpoints must be sorted and unique: get_previous_checkpoint relies on order,
        // and duplicates would make the same checkpoint checkable twice.
        let mut checkpoints = config.checkpoints.clone();
        checkpoints.sort_unstable();
        checkpoints.dedup();
        Self {
            config: DeadLoopDetectorConfig {
                checkpoints,
                ..config
            },
            checked_checkpoints: std::collections::HashSet::new(),
        }
    }

    /// Detect dead loops in reasoning content.
    pub fn detect(&mut self, reasoning_message: &str) -> DeadLoopDetectionResult {
        // Checkpoints are defined in characters, not bytes (CJK chars are 3 bytes each).
        let char_count = reasoning_message.chars().count() as u32;

        for &checkpoint in &self.config.checkpoints {
            if char_count >= checkpoint && !self.checked_checkpoints.contains(&checkpoint) {
                self.checked_checkpoints.insert(checkpoint);

                if let Some(result) = self.detect_at_checkpoint(reasoning_message, checkpoint) {
                    return result;
                }
            }
        }

        DeadLoopDetectionResult {
            detected: false,
            loop_type: None,
            details: None,
        }
    }

    /// Reset detector state (called at the start of a new API request).
    pub fn reset(&mut self) {
        self.checked_checkpoints.clear();
    }

    fn detect_at_checkpoint(&self, text: &str, checkpoint: u32) -> Option<DeadLoopDetectionResult> {
        let previous_checkpoint = self.get_previous_checkpoint(checkpoint);
        // Checkpoints are character counts; map to a safe byte boundary for slicing.
        // Never slice at the raw checkpoint value (it may land inside a multi-byte char).
        let start_index = match previous_checkpoint {
            Some(prev) => text
                .char_indices()
                .nth(prev as usize)
                .map(|(byte_index, _)| byte_index)
                .unwrap_or(text.len()),
            None => 0,
        };
        let segment = if start_index < text.len() {
            &text[start_index..]
        } else {
            return None;
        };

        // Type 3: Short sequence loop detection (checkpoint 1 only)
        if checkpoint == self.config.checkpoints[0] {
            if let Some(result) = self.detect_short_sequence(segment) {
                return Some(result);
            }
        }

        // Type 1 and Type 2: executed at checkpoints 2 and 3
        if self.config.checkpoints.len() >= 2 && checkpoint >= self.config.checkpoints[1] {
            // Type 1: Paragraph content duplication detection
            if let Some(result) = self.detect_paragraph_repeat(segment) {
                return Some(result);
            }

            // Type 2: Ordered list duplicate detection
            if let Some(result) = self.detect_list_repeat(segment) {
                return Some(result);
            }
        }

        None
    }

    /// Scan the whole segment for short sequence loops.
    /// The segment is the slice between two checkpoints, passed in by the caller;
    /// an inner window is redundant here because checkpoints already bound the segment.
    fn detect_short_sequence(&self, text: &str) -> Option<DeadLoopDetectionResult> {
        // Check for short sequence loops without regex (Rust regex doesn't support backreferences).
        // Work in char space: unit positions are char counts, and slicing the raw string
        // could land inside a multi-byte char for CJK text.
        let chars: Vec<char> = text.chars().collect();
        let min_unit = self.config.min_repeat_unit_length as usize;
        let min_count = self.config.min_repeat_count as usize;

        for unit_len in min_unit..=chars.len() / min_count {
            for start in 0..=chars.len() - unit_len * min_count {
                let unit = &chars[start..start + unit_len];
                let mut consecutive = 1;
                for i in 1..min_count {
                    let next_start = start + i * unit_len;
                    if next_start + unit_len <= chars.len()
                        && &chars[next_start..next_start + unit_len] == unit
                    {
                        consecutive += 1;
                    } else {
                        break;
                    }
                }
                if consecutive >= min_count {
                    return Some(DeadLoopDetectionResult {
                        detected: true,
                        loop_type: Some(LoopType::ShortSequence),
                        details: Some(format!(
                            "Detected short sequence loop: \"{}\" repeated {} times",
                            unit.iter().collect::<String>(),
                            consecutive
                        )),
                    });
                }
            }
        }

        None
    }

    fn detect_paragraph_repeat(&self, text: &str) -> Option<DeadLoopDetectionResult> {
        let blocks = self.split_into_semantic_blocks(text);

        if blocks.len() < self.config.min_period_elements as usize {
            return None;
        }

        if let Some(period) = self.detect_period(&blocks) {
            return Some(DeadLoopDetectionResult {
                detected: true,
                loop_type: Some(LoopType::ParagraphRepeat),
                details: Some(format!("Detected paragraph repeat with period {}", period)),
            });
        }

        None
    }

    fn detect_list_repeat(&self, text: &str) -> Option<DeadLoopDetectionResult> {
        let lines: Vec<&str> = text.split('\n').collect();

        if lines.len() < self.config.min_period_elements as usize {
            return None;
        }

        let normalized: Vec<String> = lines
            .iter()
            .map(|line| self.normalize_list_item(line))
            .collect();

        if let Some(period) = self.detect_period(&normalized) {
            return Some(DeadLoopDetectionResult {
                detected: true,
                loop_type: Some(LoopType::ListRepeat),
                details: Some(format!("Detected list repeat with period {}", period)),
            });
        }

        None
    }

    fn detect_period<T: PartialEq>(&self, elements: &[T]) -> Option<usize> {
        let max_period = std::cmp::min(self.config.max_period_length as usize, elements.len() / 2);

        for period in 1..=max_period {
            let mut consecutive_count = 0;

            for i in (period..elements.len()).rev() {
                if elements[i] == elements[i - period] {
                    consecutive_count += 1;
                } else {
                    break;
                }
            }

            if consecutive_count >= self.config.min_period_elements as usize {
                return Some(period);
            }
        }

        None
    }

    fn split_into_semantic_blocks(&self, text: &str) -> Vec<String> {
        let separators = ['.', '!', '!', ';', ';', '?', '?', '\n'];
        text.split(|c| separators.contains(&c))
            .filter(|block| !block.trim().is_empty())
            .map(|s| s.to_string())
            .collect()
    }

    fn normalize_list_item(&self, line: &str) -> String {
        // Remove ordered list labels: "1. ", "2. ", "10. ", etc.
        let trimmed = line.trim();
        if let Some(pos) = trimmed.find(". ") {
            let prefix = &trimmed[..pos];
            if prefix.chars().all(|c| c.is_ascii_digit()) {
                return trimmed[pos + 2..].to_string();
            }
        }
        trimmed.to_string()
    }

    fn get_previous_checkpoint(&self, current: u32) -> Option<u32> {
        let index = self.config.checkpoints.iter().position(|&c| c == current)?;
        if index == 0 {
            None
        } else {
            Some(self.config.checkpoints[index - 1])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn short_checkpoint_config(checkpoints: Vec<u32>) -> DeadLoopDetectorConfig {
        DeadLoopDetectorConfig {
            checkpoints,
            ..DeadLoopDetectorConfig::default()
        }
    }

    #[test]
    fn test_short_sequence_detection() {
        let mut detector = DeadLoopDetector::new(short_checkpoint_config(vec![10]));
        let text = "test test test test test"; // repeated "test "
        let result = detector.detect(text);
        assert!(result.detected);
        assert!(matches!(result.loop_type, Some(LoopType::ShortSequence)));
    }

    #[test]
    fn test_no_false_positive() {
        let mut detector = DeadLoopDetector::default();
        let text = "This is a normal reasoning message without loops.";
        let result = detector.detect(text);
        assert!(!result.detected);
    }

    #[test]
    fn test_reset() {
        let mut detector = DeadLoopDetector::new(short_checkpoint_config(vec![10]));
        detector.detect("test test test test");
        assert!(!detector.checked_checkpoints.is_empty());
        detector.reset();
        assert!(detector.checked_checkpoints.is_empty());
    }

    #[test]
    fn test_cjk_byte_boundary_no_panic() {
        // 26 distinct CJK chars = 78 bytes: byte length crosses checkpoint 20 while
        // char length does not reach checkpoint 20 until later. Previously the detector
        // sliced at raw byte offsets (e.g. &text[10..] lands mid-char -> panic).
        let mut detector = DeadLoopDetector::new(short_checkpoint_config(vec![10, 20]));
        let text = "甲乙丙丁戊己庚辛壬癸子丑寅卯辰巳午未申酉戌亥天地玄黄";
        assert_eq!(text.len(), 78); // 3 bytes per char
        assert_eq!(text.chars().count(), 26);
        let result = detector.detect(text);
        assert!(!result.detected);
        assert!(result.loop_type.is_none());
    }

    #[test]
    fn test_segment_short_sequence_detected_immediately() {
        // Streaming simulation: after crossing checkpoint 1, the full segment (not just
        // the trailing 200 chars) is scanned. The repeat below lies before the trailing
        // window of the old implementation and was previously missed.
        let mut detector = DeadLoopDetector::new(short_checkpoint_config(vec![500]));
        let alphabet = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
        let tail: String = alphabet.chars().cycle().take(200).collect();

        let before = "abc".repeat(100); // 300 chars, below checkpoint
        assert!(!detector.detect(&before).detected);

        let text = format!("{before}{tail}"); // 500 chars, repeat lies in chars 0..300
        let result = detector.detect(&text);
        assert!(result.detected);
        assert!(matches!(result.loop_type, Some(LoopType::ShortSequence)));
    }

    #[test]
    fn test_checkpoints_sorted_and_deduped() {
        let detector = DeadLoopDetector::new(short_checkpoint_config(vec![2000, 500, 500, 1000]));
        assert_eq!(detector.config.checkpoints, vec![500, 1000, 2000]);
    }
}
