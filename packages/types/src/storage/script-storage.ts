/**
 * Script Storage Type Definitions
 * Defining metadata and query options related to script persistent storage
 */

import type { Timestamp } from "../common.js";

/**
 * Script Storage Metadata
 * Metadata information for indexing and querying
 */
export interface ScriptStorageMetadata {
  /** Script name */
  name: string;
  /** Script description */
  description?: string;
  /** categorization */
  category?: string;
  /** tagged array */
  tags?: string[];
  /** Enable or not */
  enabled?: boolean;
  /** Creation time */
  createdAt?: Timestamp;
  /** update time */
  updatedAt?: Timestamp;
}