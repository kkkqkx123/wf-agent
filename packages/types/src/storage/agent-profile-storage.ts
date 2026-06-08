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