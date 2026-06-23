/**
 * Workflow Checkpoint Module
 *
 * Provides checkpoint functionality for workflow execution.
 */

// Coordinator
export {
  CheckpointCoordinator,
  type CheckpointDependencies,
  type CheckpointOptions,
  type CreateCheckpointOptions,
} from "./checkpoint-coordinator.js";

// State Manager
export { CheckpointState } from "./checkpoint-state-manager.js";

// Config Resolver
export {
  WorkflowCheckpointConfigResolver,
  buildNodeCheckpointLayers,
  resolveCheckpointConfig,
  shouldCreateCheckpoint,
  getCheckpointDescription,
} from "./utils/config-resolver.js";

// Checkpoint Policy (Workflow-specific)
export {
  WorkflowCheckpointTrigger,
  type WorkflowCheckpointPolicy,
  DEFAULT_WORKFLOW_CHECKPOINT_POLICY,
  MINIMAL_WORKFLOW_CHECKPOINT_POLICY,
  COMPREHENSIVE_WORKFLOW_CHECKPOINT_POLICY,
  NO_WORKFLOW_CHECKPOINT_POLICY,
} from "./workflow-checkpoint-policy.js";
