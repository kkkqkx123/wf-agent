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

/**
 * Node template list query options
 */
export interface NodeTemplateListOptions {
  /** Name fuzzy search */
  nameContains?: string;
  /** Template type filter (single or multiple) */
  type?: string | string[];
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
  sortBy?: 'createdAt' | 'updatedAt' | 'type' | 'name';
  /** Sort order */
  sortOrder?: 'asc' | 'desc';
  /** Pagination offset */
  offset?: number;
  /** Pagination limit */
  limit?: number;
}