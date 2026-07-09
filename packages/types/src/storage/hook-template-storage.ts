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

/**
 * Hook template list query options
 */
export interface HookTemplateListOptions {
  /** Name fuzzy search */
  nameContains?: string;
  /** Hook type filter (single or multiple) */
  hookType?: string | string[];
  /** Category filter (single or multiple) */
  category?: string | string[];
  /** Tag filter (any match) */
  tags?: string[];
  /** Description fuzzy search */
  descriptionContains?: string;
  /** Created after timestamp */
  createdAfter?: Timestamp;
  /** Created before timestamp */
  createdBefore?: Timestamp;
  /** Updated after timestamp */
  updatedAfter?: Timestamp;
  /** Updated before timestamp */
  updatedBefore?: Timestamp;
  /** Sort field */
  sortBy?: 'createdAt' | 'updatedAt' | 'hookType' | 'name';
  /** Sort order */
  sortOrder?: 'asc' | 'desc';
  /** Pagination offset */
  offset?: number;
  /** Pagination limit */
  limit?: number;
}