/**
 * Core Checkpoint Constants
 *
 * Shared constants used across checkpoint implementations.
 */

import type { DeltaStorageConfig } from "@wf-agent/types";

/**
 * Default delta storage configuration
 */
export const DEFAULT_DELTA_STORAGE_CONFIG: DeltaStorageConfig = {
  enabled: true,
  baselineInterval: 10,
  maxDeltaChainLength: 20,
};
