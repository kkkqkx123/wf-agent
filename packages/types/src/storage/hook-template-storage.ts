/**
 * Hook Template Storage Type Definitions
 * Defining metadata and query options related to hook template persistent storage
 */

import type { Timestamp } from "../common.js";

/**
 * Hook Template Storage Metadata
 * Metadata information for indexing and querying
 */
export interface HookTemplateStorageMetadata {
  /** Hook template name */
  name: string;
  /** Hook type */
  hookType: string;
  /** Hook template description */
  description?: string;
  /** categorization */
  category?: string;
  /** tagged array */
  tags?: string[];
  /** Creation time */
  createdAt: Timestamp;
  /** update time */
  updatedAt: Timestamp;
}