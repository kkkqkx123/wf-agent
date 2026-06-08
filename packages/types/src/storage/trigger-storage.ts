/**
 * Trigger Storage Type Definitions
 * Defining metadata and query options related to trigger template persistent storage
 */

import type { Timestamp } from "../common.js";

/**
 * Trigger Template Storage Metadata
 * Metadata information for indexing and querying
 */
export interface TriggerStorageMetadata {
  /** Trigger template name */
  name: string;
  /** Trigger description */
  description?: string;
  /** categorization */
  category?: string;
  /** tagged array */
  tags?: string[];
  /** Enable or not */
  enabled?: boolean;
  /** Creation time */
  createdAt: Timestamp;
  /** update time */
  updatedAt: Timestamp;
}