/**
 * Agent Loop Checkpoint Policy
 *
 * Defines the checkpoint behavior specific to Agent Loop execution.
 * Agent checkpoints capture the agent's loop state, messages, and model interactions.
 */

/**
 * Agent checkpoint trigger events
 */
export enum AgentCheckpointTrigger {
  /** Create checkpoint after each iteration */
  ON_ITERATION = "on_iteration",
  /** Create checkpoint when agent loop completes */
  ON_COMPLETE = "on_complete",
  /** Create checkpoint when agent loop encounters an error */
  ON_ERROR = "on_error",
  /** Create checkpoint when agent is paused */
  ON_PAUSE = "on_pause",
  /** Create checkpoint when a tool is called */
  ON_TOOL_CALL = "on_tool_call",
  /** Create checkpoint when tool results are received */
  ON_TOOL_RESULT = "on_tool_result",
  /** Only create checkpoints manually */
  MANUAL = "manual",
  /** Disable automatic checkpointing */
  NEVER = "never",
}

/**
 * Agent Loop Checkpoint Policy Configuration
 *
 * Controls when and how checkpoints are created during agent loop execution.
 */
export interface AgentCheckpointPolicy {
  /** Whether automatic checkpointing is enabled */
  enabled: boolean;

  /** Which events trigger checkpoint creation */
  trigger: AgentCheckpointTrigger | AgentCheckpointTrigger[];

  /** Content options */
  content?: {
    /** Include the complete execution state */
    includeState?: boolean;
    /** Include message history (usually true for agent) */
    includeMessages?: boolean;
  };

  /** Retention policy */
  retention?: {
    /** Maximum number of checkpoints to keep for this entity */
    maxCheckpoints?: number;
    /** Maximum age of checkpoints in milliseconds */
    maxAge?: number;
  };
}

/**
 * Default Agent Checkpoint Policy
 *
 * Creates checkpoints on important events: error, pause, completion.
 * Retains up to 1000 checkpoints for up to 7 days.
 */
export const DEFAULT_AGENT_CHECKPOINT_POLICY: AgentCheckpointPolicy = {
  enabled: true,
  trigger: [
    AgentCheckpointTrigger.ON_ERROR,
    AgentCheckpointTrigger.ON_PAUSE,
    AgentCheckpointTrigger.ON_COMPLETE,
  ],
  content: {
    includeState: true,
    includeMessages: true,
  },
  retention: {
    maxCheckpoints: 1000,
    maxAge: 7 * 24 * 60 * 60 * 1000, // 7 days
  },
};

/**
 * Minimal Agent Checkpoint Policy
 *
 * Only checkpoints on error and completion.
 * Uses compression and keeps fewer checkpoints (100 max, 1 day).
 */
export const MINIMAL_AGENT_CHECKPOINT_POLICY: AgentCheckpointPolicy = {
  enabled: true,
  trigger: [AgentCheckpointTrigger.ON_ERROR, AgentCheckpointTrigger.ON_COMPLETE],
  content: {
    includeState: true,
    includeMessages: false,
  },
  retention: {
    maxCheckpoints: 100,
    maxAge: 24 * 60 * 60 * 1000, // 1 day
  },
};

/**
 * Comprehensive Agent Checkpoint Policy
 *
 * Creates checkpoints on every event for detailed history.
 * Keeps more checkpoints for detailed debugging and time-travel capabilities.
 */
export const COMPREHENSIVE_AGENT_CHECKPOINT_POLICY: AgentCheckpointPolicy = {
  enabled: true,
  trigger: [
    AgentCheckpointTrigger.ON_ITERATION,
    AgentCheckpointTrigger.ON_ERROR,
    AgentCheckpointTrigger.ON_PAUSE,
    AgentCheckpointTrigger.ON_COMPLETE,
    AgentCheckpointTrigger.ON_TOOL_CALL,
    AgentCheckpointTrigger.ON_TOOL_RESULT,
  ],
  content: {
    includeState: true,
    includeMessages: true,
  },
  retention: {
    maxCheckpoints: 10000,
    maxAge: 30 * 24 * 60 * 60 * 1000, // 30 days
  },
};

/**
 * No Checkpoint Policy
 *
 * Disables automatic checkpointing completely.
 */
export const NO_AGENT_CHECKPOINT_POLICY: AgentCheckpointPolicy = {
  enabled: false,
  trigger: AgentCheckpointTrigger.NEVER,
};
