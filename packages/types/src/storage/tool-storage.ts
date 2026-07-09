/**
 * Tool Storage Type Definitions
 * Defining metadata and query options related to tool persistent storage
 */

/**
 * Tool Storage Metadata
 * Metadata information for indexing and querying
 */
export interface ToolStorageMetadata {
  /** Tool ID */
  toolId: string;
  /** Tool type */
  type: string;
  /** Tool description */
  description?: string;
  /** categorization */
  category?: string;
  /** tagged array */
  tags?: string[];
}

/**
 * Tool list query options
 */
export interface ToolListOptions {
  /** Exact toolId match */
  toolId?: string;
  /** Tool type filter (single or multiple) */
  type?: string | string[];
  /** Category filter (single or multiple) */
  category?: string | string[];
  /** Tag filter (any match) */
  tags?: string[];
  /** Description fuzzy search */
  descriptionContains?: string;
  /** Sort field */
  sortBy?: 'toolId' | 'type' | 'category';
  /** Sort order */
  sortOrder?: 'asc' | 'desc';
  /** Pagination offset */
  offset?: number;
  /** Pagination limit */
  limit?: number;
}