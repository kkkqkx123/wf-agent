/**
 * LLM Token uses statistical type definitions
 */

/**
 * LLM Token Usage Types
 */
export interface LLMUsage {
  /** Tip token count */
  promptTokens: number;
  /** Number of completed tokens */
  completionTokens: number;
  /** Total number of tokens */
  totalTokens: number;
  /** Thinking about token counts */
  reasoningTokens?: number;
  /** Tip token cost (optional) */
  promptTokensCost?: number;
  /** Cost to complete token (optional) */
  completionTokensCost?: number;
  /** Total cost (optional) */
  totalCost?: number;
}

/**
 * Token usage history
 * Records detailed token usage for each API call
 */
export interface TokenUsageHistory {
  /** Request ID */
  requestId: string;
  /** timestamp */
  timestamp: number;
  /** Tip token count */
  promptTokens: number;
  /** Number of completed tokens */
  completionTokens: number;
  /** Total number of tokens */
  totalTokens: number;
  /** Cost (optional) */
  cost?: number;
  /** Model name (optional) */
  model?: string;
  /** Raw usage data */
  rawUsage?: LLMUsage;
}

/**
 * Token usage statistics
 * Contains token usage information for a single API call.
 */
export interface TokenUsageStats {
  /** Tip Token Count */
  promptTokens: number;
  /** Number of completed tokens */
  completionTokens: number;
  /** Total number of tokens */
  totalTokens: number;
  /** Details of the original API response */
  rawUsage?: unknown;
}

/**
 * Token usage statistics
 * Provides statistical analysis of historical records
 */
export interface TokenUsageStatistics {
  /** Total requests */
  totalRequests: number;
  /** Average number of tokens */
  averageTokens: number;
  /** Maximum number of tokens */
  maxTokens: number;
  /** Minimum number of tokens */
  minTokens: number;
  /** total cost */
  totalCost: number;
  /** Total number of hint tokens */
  totalPromptTokens: number;
  /** Total number of completed tokens */
  totalCompletionTokens: number;
}

/**
 * Token counting results
 * Response for countTokens API
 */
export interface TokenCountResult {
  /** Enter the number of tokens */
  inputTokens: number;
  /** Raw Response Data */
  raw?: unknown;
}
