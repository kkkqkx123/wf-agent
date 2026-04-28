/**
 * Metadata tool functions
 * Provide functionality for querying and merging metadata.
 */

import type { Metadata } from "@wf-agent/types";

/**
 * Get the metadata value
 */
export function getMetadata(metadata: Metadata, key: string): unknown {
  return metadata[key];
}

/**
 * Check if it exists
 */
export function hasMetadata(metadata: Metadata, key: string): boolean {
  return key in metadata;
}

/**
 * Merge metadata
 * The subsequent metadata will overwrite the previous metadata with the same key.
 */
export function mergeMetadata(...metadatas: (Metadata | Record<string, unknown>)[]): Metadata {
  return Object.assign({}, ...metadatas) as Metadata;
}
