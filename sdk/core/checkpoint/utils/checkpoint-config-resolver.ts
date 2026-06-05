/**
 * Universal Checkpoint Configuration Parser
 *
 * Re-export from sdk/api/shared/config/processors/checkpoint-config-resolver.ts for backward compatibility.
 * The canonical location for checkpoint config resolution logic is now in the API layer.
 *
 * Deprecated: Import directly from sdk/api/shared/config/processors/checkpoint-config-resolver.js
 * or from sdk/api/shared/config/index.js in new code.
 */

export {
  CheckpointConfigResolver,
  shouldCreateCheckpoint,
  getCheckpointDescription,
} from "../../../api/shared/config/processors/checkpoint-config.js";

export type {
  ConfigLayer,
  ConfigResolverOptions,
} from "../../../api/shared/config/processors/checkpoint-config.js";