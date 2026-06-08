/**
 * Predefined Workflow Types
 *
 * Type definitions for predefined workflows configuration and options.
 */

/**
 * Predefined workflow options
 */
export interface PredefinedWorkflowsOptions {
  /** Enable only the specified workflows (allowlist). */
  allowList?: string[];
  /** Disable the specified workflow (blocklist it). */
  blockList?: string[];
  /** Workflow-specific configuration */
  config?: {
    llmSummary?: {
      compressionPrompt?: string;
      timeout?: number;
      maxTriggers?: number;
    };
  };
}

/**
 * Predefined workflow category
 */
export type WorkflowCategory = "system" | "user" | "custom";

/**
 * Predefined workflow definition metadata
 */
export interface PredefinedWorkflowMetadata {
  category: WorkflowCategory;
  tags?: string[];
  description?: string;
}
