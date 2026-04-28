/**
 * Entity Serializers Index
 *
 * Exports all entity-specific serializers.
 */

export * from "./task-serializer.js";
export * from "./checkpoint-serializer.js";

/**
 * Register all entity serializers with the global registry
 */
export function registerAllSerializers(): void {
  registerTaskSerializer();
  registerCheckpointSerializer();
}

import { registerTaskSerializer } from "./task-serializer.js";
import { registerCheckpointSerializer } from "./checkpoint-serializer.js";
