/**
 * Agent Profile Storage Type Definitions
 * Defining metadata and query options related to agent profile persistent storage
 */

/**
 * Agent Profile Storage Metadata
 * Metadata information for indexing and querying
 */
export interface AgentProfileStorageMetadata {
  /** Profile ID */
  profileId: string;
  /** Profile name */
  name: string;
  /** Profile description */
  description?: string;
}

/**
 * Agent profile list query options
 */
export interface AgentProfileListOptions {
  /** Exact profileId match */
  profileId?: string;
  /** Name fuzzy search */
  nameContains?: string;
  /** Description fuzzy search */
  descriptionContains?: string;
  /** Sort field */
  sortBy?: 'profileId' | 'name';
  /** Sort order */
  sortOrder?: 'asc' | 'desc';
  /** Pagination offset */
  offset?: number;
  /** Pagination limit */
  limit?: number;
}