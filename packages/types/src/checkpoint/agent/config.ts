/**
 * Agent Loop Checkpoint Configuration Type Definition
 */

import type { IterationRecord } from "../../agent-execution/types.js";
import type {
  DeltaStorageConfig,
  CheckpointConfigSource,
  AgentLoopCheckpointTriggerType,
} from "../base.js";

/**
 * Agent Loop Checkpoint Configuration Context
 */
export interface AgentLoopCheckpointConfigContext {
  /** Trigger timing */
  triggerType: AgentLoopCheckpointTriggerType;
  /** Current number of iterations */
  currentIteration: number;
  /** Is there an error */
  hasError?: boolean;
  /** Iteration record */
  iterationRecord?: IterationRecord;
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
