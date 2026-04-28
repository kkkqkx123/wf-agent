/**
 * Token Estimator
 * Provides fast token estimation based on CJK/Latin character classification
 * No external dependencies, lightweight and efficient
 */

export interface TokenEstimatorConfig {
  /** CJK character token factor (default 1.0) */
  cjkFactor?: number;
  /** Latin character token factor (default 0.25, ~4 chars/token) */
  latinFactor?: number;
}

export class TokenEstimator {
  private cjkFactor: number;
  private latinFactor: number;

  constructor(config: TokenEstimatorConfig = {}) {
    this.cjkFactor = config.cjkFactor ?? 1.0;
    this.latinFactor = config.latinFactor ?? 0.25;
  }

  /**
   * Estimate the number of tokens for the given text
   */
  estimate(text: string): number {
    if (!text || text.length === 0) return 0;

    // Fast path: ASCII-only text
    if (this.isPureAscii(text)) {
      const charsPerToken = Math.floor(1 / this.latinFactor);
      return Math.ceil(text.length / charsPerToken);
    }

    let count = 0;

    // Fast count whitespace characters (each 0.5 token)
    const wsCount = (text.match(/[ \t\n]/g) || []).length;
    count += wsCount * 0.5;

    // Process each character
    for (const ch of text) {
      if (this.isWhitespace(ch)) continue;

      if (this.isCJK(ch)) {
        count += this.cjkFactor;
      } else if (this.isAscii(ch)) {
        count += this.latinFactor;
      } else {
        count += 1.0; // Other Unicode characters (emoji, symbols, etc.)
      }
    }

    return Math.round(count);
  }

  /**
   * Check if the character is CJK (Chinese/Japanese/Korean)
   */
  private isCJK(ch: string): boolean {
    const code = ch.charCodeAt(0);
    return (
      (0x4e00 <= code && code <= 0x9fff) || // CJK Unified Ideographs
      (0x3400 <= code && code <= 0x4dbf) || // CJK Extension A
      (0x20000 <= code && code <= 0x2a6df) || // CJK Extension B
      (0x3040 <= code && code <= 0x309f) || // Hiragana
      (0x30a0 <= code && code <= 0x30ff) || // Katakana
      (0xac00 <= code && code <= 0xd7af) || // Hangul Syllables
      (0x1100 <= code && code <= 0x11ff) || // Hangul Jamo
      (0x3130 <= code && code <= 0x318f) // Hangul Compatibility Jamo
    );
  }

  /**
   * Check if the character is ASCII
   */
  private isAscii(ch: string): boolean {
    return ch.charCodeAt(0) < 0x80;
  }

  /**
   * Check if the text is pure ASCII
   */
  private isPureAscii(text: string): boolean {
    for (let i = 0; i < text.length; i++) {
      if (text.charCodeAt(i) >= 0x80) {
        return false;
      }
    }
    return true;
  }

  /**
   * Check if the character is whitespace
   */
  private isWhitespace(ch: string): boolean {
    return ch === " " || ch === "\t" || ch === "\n" || ch === "\r";
  }

  /**
   * Check if the text fits within the token limit
   */
  fitsWithin(text: string, maxTokens: number): boolean {
    return this.estimate(text) <= maxTokens;
  }

  /**
   * Find the split point so that the split text is within the token limit
   */
  findSplitPoint(text: string, maxTokens: number): number {
    const maxChars = maxTokens * 4;
    if (text.length <= maxChars) return text.length;

    // Try to split at newline
    const searchRange = text.slice(Math.max(0, maxChars - 50), maxChars);
    const newlineIdx = searchRange.lastIndexOf("\n");
    if (newlineIdx !== -1) return maxChars - 50 + newlineIdx + 1;

    // Try to split at space
    const spaceIdx = searchRange.lastIndexOf(" ");
    if (spaceIdx !== -1) return maxChars - 50 + spaceIdx + 1;

    return maxChars;
  }
}

// Global default instance
export const defaultEstimator = new TokenEstimator();

// Convenience function
export function estimateTokens(text: string): number {
  return defaultEstimator.estimate(text);
}
