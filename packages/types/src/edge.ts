/**
 * Edge Type Definition
 * Defines the connection relationship between nodes in a workflow
 */

import type { ID, Metadata } from "./common.js";
import type { Condition } from "./graph/condition.js";

/**
 * side type
 */
export type EdgeType =
  /** The default side, the unconditional connection, can always be made through the */
  | "DEFAULT"
  /** Conditional side, requires a condition assessment and fulfillment of the condition to be passed */
  | "CONDITIONAL";

/**
 * Side Condition Type (using the harmonized Condition type)
 */
export type EdgeCondition = Condition;

/**
 * Edge metadata type
 */
export interface EdgeMetadata {
  /** tagged array */
  tags?: string[];
  /** Custom Field Objects */
  customFields?: Metadata;
}

/**
 * Edge Definition Types
 */
export interface Edge {
  /** edge unique identifier */
  id: ID;
  /** Source node ID */
  sourceNodeId: ID;
  /** Target Node ID */
  targetNodeId: ID;
  /** side type */
  type: EdgeType;
  /** Optional conditional expression (only required for CONDITIONAL type) */
  condition?: EdgeCondition;
  /** Optional side labels */
  label?: string;
  /** Optional side descriptions */
  description?: string;
  /** Edge weights for sorting when multiple conditional edges are satisfied at the same time (the larger the value the higher the priority) */
  weight?: number;
  /** Optional metadata */
  metadata?: EdgeMetadata;
}
