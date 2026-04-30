/**
 * Entity Identity Types
 *
 * Defines types for identifying and tracking execution entities
 * (WorkflowExecution, Agent, Subgraph) in the component message system.
 */

/**
 * Entity Type
 */
export type EntityType = "workflowExecution" | "agent" | "subgraph";

/**
 * Parallel Group Information
 * Used in Fork/Join scenarios to track parallel execution branches.
 */
export interface ParallelGroupInfo {
  /** Parallel group ID */
  groupId: string;

  /** Branch index (0-based) */
  branchIndex: number;

  /** Total number of branches */
  totalBranches: number;
}

/**
 * Entity Identity
 * Identifies the source entity of a message and its position in the execution hierarchy.
 */
export interface EntityIdentity {
  /** Entity type */
  type: EntityType;

  /** Entity instance ID */
  id: string;

  /** Parent entity ID (if called by another entity) */
  parentId?: string;

  /** Root entity ID (topmost entity in the hierarchy) */
  rootId: string;

  /** Nesting depth (0 for root entities) */
  depth: number;

  /** Parallel group information (for Fork/Join scenarios) */
  parallelGroup?: ParallelGroupInfo;
}

/**
 * Message Trace Information
 * Used for debugging and correlating related messages.
 */
export interface MessageTrace {
  /** Call chain (array of entity IDs) */
  chain: string[];

  /** Sequence number (for ordering) */
  sequence: number;

  /** Correlation ID (links related messages) */
  correlationId?: string;
}

/**
 * Entity Context
 * Extended entity information with runtime state.
 */
export interface EntityContext extends EntityIdentity {
  /** Creation timestamp */
  createdAt: number;

  /** Entity status */
  status: "running" | "paused" | "completed" | "error" | "cancelled";

  /** Additional metadata */
  metadata?: Record<string, unknown>;
}

/**
 * Helper function to create a root entity identity
 */
export function createRootEntityIdentity(
  type: EntityType,
  id: string
): EntityIdentity {
  return {
    type,
    id,
    rootId: id,
    depth: 0,
  };
}

/**
 * Helper function to create a child entity identity
 */
export function createChildEntityIdentity(
  type: EntityType,
  id: string,
  parent: EntityIdentity
): EntityIdentity {
  return {
    type,
    id,
    parentId: parent.id,
    rootId: parent.rootId,
    depth: parent.depth + 1,
    parallelGroup: parent.parallelGroup,
  };
}

/**
 * Helper function to create a fork branch entity identity
 */
export function createForkBranchEntityIdentity(
  type: EntityType,
  id: string,
  parent: EntityIdentity,
  groupId: string,
  branchIndex: number,
  totalBranches: number
): EntityIdentity {
  return {
    type,
    id,
    parentId: parent.id,
    rootId: parent.rootId,
    depth: parent.depth + 1,
    parallelGroup: {
      groupId,
      branchIndex,
      totalBranches,
    },
  };
}
