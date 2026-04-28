/**
 * Workflow relationship type definition
 */

import type { ID } from "../common.js";

/**
 * Workflow Relationship Information
 * Used to maintain parent-child relationship chains between workflows
 */
export interface WorkflowRelationship {
  /** Workflow ID */
  workflowId: ID;
  /** Parent workflow ID (if any) */
  parentWorkflowId?: ID;
  /** List of sub workflow IDs */
  childWorkflowIds: Set<ID>;
  /** SUBGRAPH node ID mapping that references this workflow */
  referencedBy: Map<ID, ID>; // key: SUBGRAPH node ID, value: parent workflow ID
  /** Relationship Depth */
  depth: number;
}

/**
 * Workflow hierarchy information
 */
export interface WorkflowHierarchy {
  /** Ancestor chain (from root to parent) */
  ancestors: ID[];
  /** Chain of descendants (from children to grandchildren) */
  descendants: ID[];
  /** Depth in the hierarchy */
  depth: number;
  /** Root workflow ID */
  rootWorkflowId: ID;
}
