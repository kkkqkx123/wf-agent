/**
 * Workflow Configuration Type Definitions
 */

import type { ToolApprovalOptions } from "../tool/approval.js";
import type { LLMMessage } from "../message/message.js";

/**
 * Checkpoint Configuration Types
 * Define checkpoint creation policies and behaviors
 */
export interface CheckpointConfig {
  /** Whether to enable checkpoints (global switch) */
  enabled?: boolean;
  /** Whether to create checkpoints before node execution (global default behavior) */
  checkpointBeforeNode?: boolean;
  /** Whether to create checkpoints after node execution (global default behavior) */
  checkpointAfterNode?: boolean;
}

/**
 * Triggered subworkflow-specific configuration types
 * Define special behavior options for triggered subworkflows
 */
export interface TriggeredSubworkflowConfig {
  /** Whether to enable checkpoints (default false) */
  enableCheckpoints?: boolean;
  /** Checkpoint configuration (if enableCheckpoints is true) */
  checkpointConfig?: CheckpointConfig;
  /** Execution timeout (milliseconds) */
  timeout?: number;
  /** Maximum number of retries */
  maxRetries?: number;
}

/**
 * Workflow Configuration Types
 * Define behavioral options for workflow execution
 */
export interface WorkflowConfig {
  /** Execution timeout (milliseconds) */
  timeout?: number;
  /** Maximum number of execution steps */
  maxSteps?: number;
  /** Whether or not to enable checkpointing (to preserve backward compatibility) */
  enableCheckpoints?: boolean;
  /** Checkpoint configuration (new) */
  checkpointConfig?: CheckpointConfig;
  /** Retry Policy Configuration */
  retryPolicy?: {
    maxRetries?: number;
    retryDelay?: number;
    backoffMultiplier?: number;
  };
  /** Tool Approval Configuration (uses ToolApprovalOptions for advanced features) */
  toolApproval?: ToolApprovalOptions;
  
  /**
   * Available Tools Configuration
   * Defines the complete set of tools that can be used during workflow execution,
   * with support for initial availability and dynamic visibility control.
   */
  availableTools?: import('./tool-config.js').AvailableTools;
  
  /**
   * Initial messages for the 'current' context
   * 
   * Pre-populated into the 'current' conversation context on execution start.
   * Messages can have any role ('system', 'user', 'assistant', etc.).
   * This replaces the previous separate 'system' context approach.
   */
  initialMessages?: LLMMessage[];
  
  /**
   * Predefined static contexts
   */
  staticContexts?: {
    id: string;
    messages: LLMMessage[];
    description?: string;
  }[];
}
