/**
 * Universal Checkpoint System
 *
 * Provides a unified checkpoint architecture that eliminates code duplication
 * between Workflow and Agent checkpoint implementations.
 */

// Core abstractions
export { BaseCheckpointCoordinator } from "./base-checkpoint-coordinator.js";
export { BaseCheckpointStateManager } from "./base-checkpoint-state-manager.js";
export { BaseDiffCalculator } from "./base-diff-calculator.js";
export { BaseDeltaRestorer } from "./base-delta-restorer.js";

// Metrics
export { CheckpointMetricsCollector } from "./checkpoint-metrics-collector.js";

// Types and interfaces
export type {
  CheckpointStorageAdapter,
  CheckpointableEntity,
  CheckpointDependencies,
  DeltaRestoreResult,
} from "./types.js";

// Storage Adapters
export { LayertwineCheckpointAdapter } from "./adapters/index.js";
