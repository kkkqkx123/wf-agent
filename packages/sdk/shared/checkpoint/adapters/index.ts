/**
 * Checkpoint Storage Adapters
 *
 * Generic implementations of CheckpointDependencies for different storage backends
 * The generic LayertwineCheckpointAdapter supports both Agent and Workflow checkpoints
 */

export { LayertwineCheckpointAdapter } from "./layertwine-checkpoint-adapter.js";

/**
 * Type aliases for clarity when using the generic adapter
 *
 * Usage:
 *   import { LayertwineCheckpointAdapter } from './adapters'
 *   import type { AgentLoopCheckpoint, Checkpoint } from '@wf-agent/types'
 *
 *   // For Agent
 *   const agentAdapter = new LayertwineCheckpointAdapter<AgentLoopCheckpoint>(executor)
 *
 *   // For Workflow
 *   const workflowAdapter = new LayertwineCheckpointAdapter<Checkpoint>(executor)
 */
