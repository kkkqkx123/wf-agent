/**
 * Entity Serializers Index
 *
 * Exports all entity-specific serializers.
 */

export * from "./task-serializer.js";
export * from "./checkpoint-serializer.js";
export * from "./agent-loop-checkpoint-serializer.js";
export * from "./agent-loop-entity-serializer.js";

/**
 * Register all entity serializers with the global registry
 */
export function registerAllSerializers(): void {
  registerTaskSerializer();
  registerWorkflowCheckpointSerializer();
  registerAgentLoopCheckpointSerializer();
  registerAgentLoopEntitySerializer();
}

import { registerTaskSerializer } from "./task-serializer.js";
import { registerWorkflowCheckpointSerializer } from "./checkpoint-serializer.js";
import { registerAgentLoopCheckpointSerializer } from "./agent-loop-checkpoint-serializer.js";
import { registerAgentLoopEntitySerializer } from "./agent-loop-entity-serializer.js";
