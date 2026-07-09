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

/**
 * Script list query options
 */
export interface ScriptListOptions {
  /** Name fuzzy search */
  nameContains?: string;
  /** Category filter (single or multiple) */
  category?: string | string[];
  /** Tag filter (any match) */
  tags?: string[];
  /** Enabled status filter */
  enabled?: boolean;
  /** Created after timestamp */
  createdAfter?: Timestamp;
  /** Created before timestamp */
  createdBefore?: Timestamp;
  /** Updated after timestamp */
  updatedAfter?: Timestamp;
  /** Updated before timestamp */
  updatedBefore?: Timestamp;
  /** Sort field */
  sortBy?: 'createdAt' | 'updatedAt' | 'name';
  /** Sort order */
  sortOrder?: 'asc' | 'desc';
  /** Pagination offset */
  offset?: number;
  /** Pagination limit */
  limit?: number;
}