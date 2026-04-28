//! Token estimation for mixed-language text
//!
//! Provides fast token count estimation supporting ASCII, CJK, and other Unicode.
//! Used across the codebase for LLM request sizing, chunking, and content compression.

/// Token estimator for mixed-language text
#[derive(Debug, Clone, Copy)]
pub struct TokenEstimator {
    /// Tokens per CJK character (default: 1.0)
    cjk_factor: f32,
    /// Tokens per Latin character (default: 0.25, ~4 chars per token)
    latin_factor: f32,
}

impl Default for TokenEstimator {
    fn default() -> Self {
        Self {
            cjk_factor: 1.0,
            latin_factor: 0.25,
        }
    }
}

impl TokenEstimator {
    /// Create a new token estimator with custom factors
    ///
    /// # Arguments
    ///
    /// * `cjk_factor` - Tokens per CJK character (default: 1.0)
    /// * `latin_factor` - Tokens per Latin character (default: 0.25, ~4 chars per token)
    ///
    /// # Example
    ///
    /// ```
    /// use code_context_engine::utils::token_estimation::TokenEstimator;
    ///
    /// // Create estimator for models with different tokenization behavior
    /// let estimator = TokenEstimator::new(0.8, 0.3);
    /// ```
    pub const fn new(cjk_factor: f32, latin_factor: f32) -> Self {
        Self {
            cjk_factor,
            latin_factor,
        }
    }

    /// Estimate token count for text using default factors
    ///
    /// This is a convenience static method that uses default tokenization factors.
    /// For custom factors, create an instance with `TokenEstimator::new()`.
    ///
    /// # Example
    ///
    /// ```
    /// use code_context_engine::utils::token_estimation::TokenEstimator;
    ///
    /// let tokens = TokenEstimator::estimate("Hello world");
    /// ```
    pub fn estimate(text: &str) -> usize {
        DEFAULT_ESTIMATOR.estimate_with_config(text)
    }

    /// Estimate token count for text with this estimator's configuration
    ///
    /// Uses fast path for ASCII-only text, otherwise processes character by character.
    pub fn estimate_with_config(&self, text: &str) -> usize {
        // Fast path: ASCII-only text
        if text.is_ascii() {
            let chars_per_token = (1.0 / self.latin_factor) as usize;
            return text.len().div_ceil(chars_per_token.max(1));
        }

        let mut count = 0.0f32;

        // Count ASCII whitespace quickly
        let bytes = text.as_bytes();
        let ascii_ws =
            count_byte(bytes, b' ') + count_byte(bytes, b'\t') + count_byte(bytes, b'\n');
        count += ascii_ws as f32 * 0.5;

        // Process each character
        for ch in text.chars() {
            if ch.is_ascii_whitespace() || ch.is_ascii_control() {
                continue;
            }

            if Self::is_cjk(ch) {
                count += self.cjk_factor;
            } else if ch.is_ascii() {
                count += self.latin_factor;
            } else {
                // Other Unicode (emoji, symbols, etc.) - count as 1 token
                count += 1.0;
            }
        }

        count.round() as usize
    }

    /// Estimate tokens for text (instance method)
    ///
    /// Convenience method that delegates to estimate_with_config.
    pub fn estimate_text(&self, text: &str) -> usize {
        self.estimate_with_config(text)
    }

    /// Check if character is CJK (Chinese, Japanese, Korean)
    #[inline]
    fn is_cjk(ch: char) -> bool {
        let code = ch as u32;
        // CJK Unified Ideographs
        (0x4E00..=0x9FFF).contains(&code)
            || (0x3400..=0x4DBF).contains(&code)
            || (0x20000..=0x2A6DF).contains(&code)
            // Hiragana & Katakana
            || (0x3040..=0x309F).contains(&code)
            || (0x30A0..=0x30FF).contains(&code)
            // Hangul (Korean)
            || (0xAC00..=0xD7AF).contains(&code)
            || (0x1100..=0x11FF).contains(&code)
            || (0x3130..=0x318F).contains(&code)
    }

    /// Estimate tokens with a custom ratio
    pub fn estimate_with_ratio(text: &str, chars_per_token: usize) -> usize {
        if chars_per_token == 0 {
            return 0;
        }
        text.len() / chars_per_token
    }

    /// Check if text fits within token limit
    pub fn fits_within(text: &str, max_tokens: usize) -> bool {
        Self::estimate(text) <= max_tokens
    }

    /// Find split point that fits within token limit
    ///
    /// Returns the byte offset where to split.
    pub fn find_split_point(text: &str, max_tokens: usize) -> usize {
        let max_chars = max_tokens * 4;
        if text.len() <= max_chars {
            return text.len();
        }

        // Try to find a good split point (newline or space)
        let search_start = max_chars.saturating_sub(50);
        let search_end = max_chars.min(text.len());

        for i in (search_start..search_end).rev() {
            if text.as_bytes()[i] == b'\n' {
                return i + 1;
            }
        }

        for i in (search_start..search_end).rev() {
            if text.as_bytes()[i] == b' ' {
                return i + 1;
            }
        }

        max_chars
    }
}

/// Count occurrences of a byte in a slice
#[inline]
fn count_byte(bytes: &[u8], target: u8) -> usize {
    bytes.iter().filter(|&&b| b == target).count()
}

/// Global default estimator
static DEFAULT_ESTIMATOR: TokenEstimator = TokenEstimator {
    cjk_factor: 1.0,
    latin_factor: 0.25,
};

/// Estimate tokens using default estimator
///
/// This is the primary function for token estimation.
pub fn estimate_tokens(text: &str) -> usize {
    TokenEstimator::estimate(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_text() {
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn test_ascii_text() {
        let text = "Hello World";
        let tokens = estimate_tokens(text);
        // "Hello World" = 11 chars, ~3 tokens
        assert!((2..=4).contains(&tokens), "tokens: {}", tokens);
    }

    #[test]
    fn test_cjk_text() {
        let text = "你好世界";
        let tokens = estimate_tokens(text);
        // 4 CJK chars = ~4 tokens
        assert!((3..=5).contains(&tokens), "tokens: {}", tokens);
    }

    #[test]
    fn test_mixed_text() {
        let text = "Hello世界";
        let tokens = estimate_tokens(text);
        // "Hello" ~1-2 tokens + "世界" ~2 tokens = ~3-4 tokens
        assert!((2..=6).contains(&tokens), "tokens: {}", tokens);
    }

    #[test]
    fn test_whitespace() {
        let text = "Hello   World";
        let tokens = estimate_tokens(text);
        assert!(tokens > 0);
    }

    #[test]
    fn test_long_ascii() {
        let text = "The quick brown fox jumps over the lazy dog";
        let tokens = estimate_tokens(text);
        // ~43 chars, ~11 tokens
        assert!((8..=15).contains(&tokens), "tokens: {}", tokens);
    }

    #[test]
    fn test_japanese() {
        let text = "こんにちは世界";
        let tokens = estimate_tokens(text);
        // 7 chars (5 hiragana + 2 kanji) = ~7 tokens
        assert!((5..=9).contains(&tokens), "tokens: {}", tokens);
    }

    #[test]
    fn test_korean() {
        let text = "안녕하세요";
        let tokens = estimate_tokens(text);
        // 5 Hangul chars = ~5 tokens
        assert!((4..=7).contains(&tokens), "tokens: {}", tokens);
    }

    #[test]
    fn test_custom_estimator() {
        // Test with custom factors for different tokenization behavior
        let estimator = TokenEstimator::new(0.8, 0.3);

        let text = "Hello";
        let tokens = estimator.estimate_with_config(text);
        // 5 chars * 0.3 = 1.5 -> rounded to 2 tokens
        assert!((1..=3).contains(&tokens), "tokens: {}", tokens);

        let text = "你好";
        let tokens = estimator.estimate_with_config(text);
        // 2 CJK chars * 0.8 = 1.6 -> rounded to 2 tokens
        assert!((1..=3).contains(&tokens), "tokens: {}", tokens);
    }

    #[test]
    fn test_custom_estimator_ascii_fast_path() {
        // Test that ASCII fast path respects latin_factor
        let estimator = TokenEstimator::new(1.0, 0.2); // 5 chars per token

        let text = "ABCDEFGHIJ"; // 10 chars
        let tokens = estimator.estimate_with_config(text);
        // 10 chars / 5 = 2 tokens
        assert_eq!(tokens, 2, "tokens: {}", tokens);
    }

    #[test]
    fn test_default_estimator_consistency() {
        // Ensure default estimator matches estimate_tokens function
        let text = "Hello 世界 123";
        let estimator = TokenEstimator::default();
        assert_eq!(estimator.estimate_text(text), estimate_tokens(text));
    }

    #[test]
    fn test_fits_within() {
        let text = "Short text";
        assert!(TokenEstimator::fits_within(text, 100));
        assert!(!TokenEstimator::fits_within(text, 1));
    }

    #[test]
    fn test_find_split_point() {
        let text = "This is a test string that is longer than the limit.";
        let split = TokenEstimator::find_split_point(text, 5);
        assert!(split < text.len());
        assert!(split > 0);
    }
}
