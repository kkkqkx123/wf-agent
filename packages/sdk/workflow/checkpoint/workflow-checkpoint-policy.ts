/**
 * Workflow Checkpoint Policy
 *
 * Defines the checkpoint behavior specific to Workflow execution.
 * Workflow checkpoints capture the execution state, node progress, and variable state.
 */

/**
 * Workflow checkpoint trigger events
 */
export enum WorkflowCheckpointTrigger {
  /** Create checkpoint after each node execution */
  ON_NODE_EXECUTION = "on_node_execution",
  /** Create checkpoint when workflow completes */
  ON_COMPLETE = "on_complete",
  /** Create checkpoint when workflow encounters an error */
  ON_ERROR = "on_error",
  /** Create checkpoint when workflow is paused */
  ON_PAUSE = "on_pause",
  /** Create checkpoint after fork node */
  AFTER_FORK = "after_fork",
  /** Create checkpoint after join node */
  AFTER_JOIN = "after_join",
  /** Only create checkpoints manually */
  MANUAL = "manual",
  /** Disable automatic checkpointing */
  NEVER = "never",
}

/**
 * Workflow Checkpoint Policy Configuration
 *
 * Controls when and how checkpoints are created during workflow execution.
 */
export interface WorkflowCheckpointPolicy {
  /** Whether automatic checkpointing is enabled */
  enabled: boolean;

  /** Which events trigger checkpoint creation */
  trigger: WorkflowCheckpointTrigger | WorkflowCheckpointTrigger[];

  /** Content options */
  content?: {
    /** Include the complete execution state */
    includeState?: boolean;
    /** Include variable values */
    includeVariables?: boolean;
  };

  /** Retention policy */
  retention?: {
    /** Maximum number of checkpoints to keep for this execution */
    maxCheckpoints?: number;
    /** Maximum age of checkpoints in milliseconds */
    maxAge?: number;
  };
}

/**
 * Default Workflow Checkpoint Policy
 *
 * Creates checkpoints on important events: error, pause, completion.
 * Does not checkpoint on every node execution to reduce storage.
 */
export const DEFAULT_WORKFLOW_CHECKPOINT_POLICY: WorkflowCheckpointPolicy = {
  enabled: true,
  trigger: [
    WorkflowCheckpointTrigger.ON_ERROR,
    WorkflowCheckpointTrigger.ON_PAUSE,
    WorkflowCheckpointTrigger.ON_COMPLETE,
  ],
  content: {
    includeState: true,
    includeVariables: true,
  },
  retention: {
    maxCheckpoints: 500,
    maxAge: 7 * 24 * 60 * 60 * 1000, // 7 days
  },
};

/**
 * Minimal Workflow Checkpoint Policy
 *
 * Only checkpoints on error and completion.
 * Minimal storage overhead.
 */
export const MINIMAL_WORKFLOW_CHECKPOINT_POLICY: WorkflowCheckpointPolicy = {
  enabled: true,
  trigger: [WorkflowCheckpointTrigger.ON_ERROR, WorkflowCheckpointTrigger.ON_COMPLETE],
  content: {
    includeState: true,
    includeVariables: false,
  },
  retention: {
    maxCheckpoints: 50,
    maxAge: 24 * 60 * 60 * 1000, // 1 day
  },
};

/**
 * Comprehensive Workflow Checkpoint Policy
 *
 * Creates checkpoints after each node and special control-flow nodes.
 * Suitable for detailed debugging and time-travel analysis.
 */
export const COMPREHENSIVE_WORKFLOW_CHECKPOINT_POLICY: WorkflowCheckpointPolicy = {
  enabled: true,
  trigger: [
    WorkflowCheckpointTrigger.ON_NODE_EXECUTION,
    WorkflowCheckpointTrigger.ON_ERROR,
    WorkflowCheckpointTrigger.ON_PAUSE,
    WorkflowCheckpointTrigger.ON_COMPLETE,
    WorkflowCheckpointTrigger.AFTER_FORK,
    WorkflowCheckpointTrigger.AFTER_JOIN,
  ],
  content: {
    includeState: true,
    includeVariables: true,
  },
  retention: {
    maxCheckpoints: 5000,
    maxAge: 30 * 24 * 60 * 60 * 1000, // 30 days
  },
};

/**
 * No Checkpoint Policy
 *
 * Disables automatic checkpointing completely.
 */
export const NO_WORKFLOW_CHECKPOINT_POLICY: WorkflowCheckpointPolicy = {
  enabled: false,
  trigger: WorkflowCheckpointTrigger.NEVER,
};
