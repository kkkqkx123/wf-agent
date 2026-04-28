/**
 * Checkpoint module
 *
 * Provides checkpoint save, restore, and diff functionality for task state management.
 *
 * All checkpoint operations are now centralized in checkpoint-service.ts for better
 * organization and clearer separation of concerns.
 *
 * @module checkpoints
 */

// Service initialization, save, restore, and diff operations (consolidated in checkpoint-service.ts)
export { getCheckpointService, checkpointSave, checkpointRestore, checkpointDiff } from "./checkpoint-service"

// Re-export types
export type { CheckpointRestoreOptions, CheckpointDiffOptions } from "./checkpoint-service"

// Deprecated: These modules are now integrated into checkpoint-service.ts
// They are kept for backward compatibility but should not be imported directly
// @deprecated Use checkpoint-service.ts exports instead
export { checkpointRestore as checkpointRestoreDeprecated } from "./checkpoint-restore"
export { checkpointDiff as checkpointDiffDeprecated } from "./checkpoint-diff"
