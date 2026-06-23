/**
 * Workflow Checkpoint Configuration Type Definition
 */

import type { ID } from "../../common.js";
import type {
  CheckpointMetadata,
  DeltaStorageConfig,
  CheckpointConfigSource,
  WorkflowCheckpointTriggerType,
} from "../base.js";
import { DEFAULT_DELTA_STORAGE_CONFIG } from "../base.js";

// Reexport the generic types
export type { CheckpointMetadata, DeltaStorageConfig };
export { DEFAULT_DELTA_STORAGE_CONFIG };

/**
 * Checkpoint configuration context
 */
export interface CheckpointConfigContext {
  /** Triggering timing */
  triggerType: WorkflowCheckpointTriggerType;
  /** Node ID (optional) */
  nodeId?: string;
  /** Tool ID (optional) */
  toolId?: ID;
  /** Whether it is a triggered sub-workflow */
  isTriggeredSubworkflow?: boolean;
  /** Whether to explicitly enable checkpoints */
  explicitEnableCheckpoint?: boolean;
}

/**
 * Workflow Checkpoint Configuration Level
 */
export interface WorkflowCheckpointConfigLayer {
  /** Configure the source */
  source: CheckpointConfigSource;
  /** Configuration content */
  config: CheckpointConfigContent;
}

/**
 * Checkpoint configuration content
 */
export interface CheckpointConfigContent {
  /** Whether to enable checkpoints */
  enabled?: boolean;
  /** Checkpoint Description */
  description?: string;
  /** Enable configuration for specific trigger moments */
  triggers?: {
    nodeBeforeExecute?: boolean;
    nodeAfterExecute?: boolean;
    toolBefore?: boolean;
    toolAfter?: boolean;
  };
}
