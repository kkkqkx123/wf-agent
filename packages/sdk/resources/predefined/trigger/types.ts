/**
 * Predefined Trigger Types
 *
 * Type definitions for predefined triggers configuration and options.
 */

/**
 * Predefined trigger options
 */
export interface PredefinedTriggersOptions {
  /** Enable only the specified triggers (allowlist). */
  allowList?: string[];
  /** Disable the specified trigger (blocklist it). */
  blockList?: string[];
  /** Trigger-specific configuration */
  config?: {
    llmSummary?: {
      compressionPrompt?: string;
      triggeredWorkflowId?: string;
      timeout?: number;
      maxTriggers?: number;
    };
  };
}

/**
 * Predefined trigger category
 */
export type TriggerCategory = "system" | "user" | "custom";

/**
 * Predefined trigger definition metadata
 */
export interface PredefinedTriggerMetadata {
  category: TriggerCategory;
  tags?: string[];
  description?: string;
}
