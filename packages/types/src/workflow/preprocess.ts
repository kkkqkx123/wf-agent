/**
 * Workflow preprocessing related type definitions
 */

import type { ID, Timestamp } from "../common.js";

/**
 * Subworkflow merge log types
 * Logs the SUBGRAPH node merge process
 */
export interface SubgraphMergeLog {
  /** Subworkflow ID */
  subworkflowId: ID;
  /** Subworkflow name */
  subworkflowName: string;
  /** SUBGRAPH node ID */
  subgraphNodeId: ID;
  /** Merged node ID mapping (original ID -> new ID) */
  nodeIdMapping: Map<ID, ID>;
  /** Merged edge ID mapping (original ID -> new ID) */
  edgeIdMapping: Map<ID, ID>;
  /** Combined timestamps */
  mergedAt: Timestamp;
}

/**
 * Types of preprocessing validation results
 */
export interface PreprocessValidationResult {
  /** Whether the validation passes or not */
  isValid: boolean;
  /** Validation Error List */
  errors: string[];
  /** Validating Warning Lists */
  warnings: string[];
  /** Verify timestamp */
  validatedAt: Timestamp;
}
