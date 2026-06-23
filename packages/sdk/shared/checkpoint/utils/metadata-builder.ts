/**
 * Checkpoint metadata utilities
 *
 * Provides consistent metadata handling across Agent and Workflow checkpoint systems
 */

import type { CheckpointMetadata, CheckpointFormatVersion } from "@wf-agent/types";
import { CURRENT_CHECKPOINT_FORMAT_VERSION } from "@wf-agent/types";
import { mergeMetadata } from "../../../utils/index.js";

/**
 * Options for building checkpoint metadata
 */
export interface BuildCheckpointMetadataOptions {
  metadata?: CheckpointMetadata;
  description?: string;
  tags?: string[];
  customFields?: Record<string, unknown>;
}

/**
 * Build standardized checkpoint metadata with version information
 *
 * Ensures consistent handling of metadata across Agent and Workflow checkpoints:
 * - Merges provided metadata with defaults
 * - Adds format version and creation timestamp
 * - Provides consistent customFields structure
 *
 * @param options Metadata building options
 * @returns Normalized CheckpointMetadata
 */
export function buildCheckpointMetadata(
  options?: BuildCheckpointMetadataOptions,
): CheckpointMetadata | undefined {
  if (!options || !options?.metadata) {
    return undefined;
  }

  const baseMetadata = options?.metadata ?? {};
  const customFields = mergeMetadata(
    baseMetadata.customFields || {},
    options?.customFields || {},
    {
      formatVersion: CURRENT_CHECKPOINT_FORMAT_VERSION,
      createdAt: Date.now(),
    },
  );

  return {
    ...baseMetadata,
    description: options?.description ?? baseMetadata.description,
    tags: options?.tags ?? baseMetadata.tags,
    customFields,
  };
}

/**
 * Extract format version from checkpoint metadata
 *
 * @param metadata Checkpoint metadata
 * @returns Format version, or current version as default
 */
export function extractFormatVersion(
  metadata?: CheckpointMetadata,
): CheckpointFormatVersion {
  return (
    (metadata?.customFields?.["formatVersion"] as CheckpointFormatVersion) ??
    CURRENT_CHECKPOINT_FORMAT_VERSION
  );
}

/**
 * Extract creation timestamp from checkpoint metadata
 *
 * @param metadata Checkpoint metadata
 * @returns Timestamp, or current time as default
 */
export function extractCreatedAt(metadata?: CheckpointMetadata): number {
  const createdAt = metadata?.customFields?.["createdAt"];
  if (typeof createdAt === "number") {
    return createdAt;
  }
  return Date.now();
}
