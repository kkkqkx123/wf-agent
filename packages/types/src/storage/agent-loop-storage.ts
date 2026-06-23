/**
 * Agent Loop Checkpoint Storage Type Definitions
 * Defines metadata, query options for agent loop checkpoint persistence
 */

import type { ID, Timestamp } from "../common.js";
import type { TCheckpointType } from "../checkpoint/index.js";
import type { AgentLoopStatus } from "../agent-execution/types.js";

/**
 * Agent Checkpoint Storage Metadata
 * Metadata information for indexing and querying agent checkpoints
 */
export interface AgentCheckpointMetadata {
  /** Agent Loop ID */
  agentLoopId: ID;
  /** Creation timestamp */
  timestamp: Timestamp;
  /** Checkpoint type (FULL or DELTA) */
  type: TCheckpointType;
  /** Version number */
  version?: number;
  /** Tagged arrays (for categorization and retrieval) */
  tags?: string[];
  /** Customizing metadata fields */
  customFields?: Record<string, unknown>;
}

/**
 * Agent Checkpoint Storage List Query Options
 * Support for filtering and pagination
 */
export interface AgentCheckpointListOptions {
  /** Filter by agent loop ID */
  agentLoopId?: ID;
  /** Filter by checkpoint type */
  type?: TCheckpointType;
  /** Filter by tag (match any tag) */
  tags?: string[];
  /** Maximum number of returns (paged) */
  limit?: number;
  /** Offset (paging) */
  offset?: number;
}

/**
 * Agent Entity Storage Metadata
 * Metadata for agent entity lifecycle management
 */
export interface AgentEntityMetadata {
  /** Agent Loop ID */
  agentLoopId: ID;
  /** Current status */
  status: AgentLoopStatus;
  /** Creation timestamp */
  createdAt: Timestamp;
  /** Last update timestamp */
  updatedAt?: Timestamp;
  /** Completion timestamp */
  completedAt?: Timestamp;
  /** Associated profile ID */
  profileId?: ID;
  /** Tagged arrays */
  tags?: string[];
  /** Custom metadata fields */
  customFields?: Record<string, unknown>;
}

/**
 * Agent Entity Storage List Query Options
 */
export interface AgentEntityListOptions {
  /** Filter by status */
  status?: AgentLoopStatus;
  /** Filter by profile ID */
  profileId?: ID;
  /** Filter by tags */
  tags?: string[];
  /** Created after timestamp */
  createdAfter?: Timestamp;
  /** Created before timestamp */
  createdBefore?: Timestamp;
  /** Maximum number of returns */
  limit?: number;
  /** Offset */
  offset?: number;
}
