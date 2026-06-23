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