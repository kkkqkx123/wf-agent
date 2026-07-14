/**
 * Agent Loop Checkpoint Configuration Type Definition
 */

import type { IterationRecord } from "../../agent-execution/types.js";
import type {
  DeltaStorageConfig,
  CheckpointConfigSource,
  CheckpointTriggerType,
} from "../base.js";

/**
 * Agent Loop Checkpoint Content Configuration
 * Controls what data is included in the checkpoint
 */
export interface AgentCheckpointContentConfig {
  /** Whether to include execution state (default: true) */
  includeState?: boolean;
  /** Whether to include message history (default: false) */
  includeMessages?: boolean;
  /** Limit message history to N most recent messages */
  messageLimit?: number;
  /** Whether to include tool calls (default: true) */
  includeToolCalls?: boolean;
  /** Limit tool calls to N most recent calls */
  toolCallLimit?: number;
}

/**
 * Agent Loop Checkpoint Configuration Context
 */
export interface AgentLoopCheckpointConfigContext {
  /** Trigger timing (using unified CheckpointTrigger enum) */
  triggerType: CheckpointTriggerType;
  /** Current number of iterations */
  currentIteration: number;
  /** Is there an error */
  hasError?: boolean;
  /** Iteration record */
  iterationRecord?: IterationRecord;
  /** Content configuration to apply */
  contentConfig?: AgentCheckpointContentConfig;
}

/**
 * Agent Loop Checkpoint Configuration
 */
export interface AgentLoopCheckpointConfig {
  /** Whether to enable checkpoints */
  enabled?: boolean;
  /** Checkpoint interval (created every N iterations) */
  interval?: number;
  /** Whether to create only on error */
  onErrorOnly?: boolean;
  /** Incremental Storage Configuration */
  deltaStorage?: Partial<DeltaStorageConfig>;
  /** Content configuration for what to include in checkpoint */
  content?: AgentCheckpointContentConfig;
}

/**
 * Agent Loop Checkpoint Configuration Hierarchy
 */
export interface AgentLoopCheckpointConfigLayer {
  /** Configuration Sources */
  source: CheckpointConfigSource;
  /** Deployment Details */
  config: AgentLoopCheckpointConfig;
}
