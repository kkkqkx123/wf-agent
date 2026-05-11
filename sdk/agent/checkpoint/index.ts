/**
 * Agent Loop checkpoint module is exported uniformly.
 */

// Configure the parser
export { AgentLoopCheckpointResolver } from "./checkpoint-config-resolver.js";

// Checkpoint Coordinator
export {
  AgentLoopCheckpointCoordinator,
  type CheckpointDependencies,
  type CheckpointOptions,
} from "./checkpoint-coordinator.js";

// State Manager (NEW - Phase 4)
export { AgentLoopCheckpointStateManager } from "./agent-loop-checkpoint-state-manager.js";

// Utility functions
export {
  createCheckpoint,
  restoreFromCheckpoint,
  type CreateCheckpointOptions,
} from "./checkpoint-utils.js";
