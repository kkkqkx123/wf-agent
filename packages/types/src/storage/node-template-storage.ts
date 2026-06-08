/**
 * Node Template Storage Type Definitions
 * Defining metadata and query options related to node template persistent storage
 */

import type { Timestamp } from "../common.js";

/**
 * Node Template Storage Metadata
 * Metadata information for indexing and querying
 */
export interface NodeTemplateStorageMetadata {
  /** Node template name */
  name: string;
  /** Node type */
  type: string;
  /** Node template description */
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