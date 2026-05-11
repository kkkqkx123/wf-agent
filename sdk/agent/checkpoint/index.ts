/**
 * Agent Loop checkpoint module is exported uniformly.
 */

// Configure the parser
export { AgentLoopCheckpointConfigResolver } from "./utils/config-resolver.js";

// Checkpoint Coordinator
export {
  AgentLoopCheckpointCoordinator,
  type CheckpointDependencies,
  type CheckpointOptions,
} from "./checkpoint-coordinator.js";

// State Manager (NEW - Phase 4)
export { AgentLoopCheckpointStateManager } from "./checkpoint-state-manager.js";

// Utility functions
export {
  createCheckpoint,
  restoreFromCheckpoint,
  type CreateCheckpointOptions,
} from "./utils/checkpoint-utils.js";
