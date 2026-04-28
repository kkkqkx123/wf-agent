/**
 * Workflow Configuration Type Definitions
 */

/**
 * Tool Approval Configuration
 * Define approval policies for tool invocations in a workflow
 */
export interface ToolApprovalConfig {
  /**
   * List of automatically approved tools (whitelist)
   * Array of tool IDs or names, these tool calls do not require manual approval
   */
  autoApprovedTools: string[];
}

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
  /** Tool Approval Configuration */
  toolApproval?: ToolApprovalConfig;
}
