/**
 * Subgraph Message Types
 *
 * Messages related to subgraph (nested workflow) execution.
 */

/**
 * Subgraph Message Type
 */
export enum SubgraphMessageType {
  /** Subgraph started */
  START = "subgraph.start",

  /** Subgraph ended */
  END = "subgraph.end",

  /** Context inherited from parent */
  CONTEXT_INHERIT = "subgraph.context.inherit",

  /** Context returned to parent */
  CONTEXT_RETURN = "subgraph.context.return",

  /** State synchronized with parent */
  STATE_SYNC = "subgraph.state.sync",
}

/**
 * Subgraph Start Data
 */
export interface SubgraphStartData {
  /** Subgraph execution ID */
  subthreadId: string;

  /** Parent execution ID */
  parentThreadId: string;

  /** Root execution ID */
  rootThreadId: string;

  /** Subgraph graph ID */
  graphId: string;

  /** Nesting depth */
  depth: number;

  /** Inherited variables from parent */
  inheritedVariables: Record<string, unknown>;

  /** Node ID in parent graph */
  nodeId: string;
}

/**
 * Subgraph End Data
 */
export interface SubgraphEndData {
  /** Subgraph execution ID */
  subthreadId: string;

  /** Parent execution ID */
  parentThreadId: string;

  /** Final status */
  status: "completed" | "failed" | "cancelled";

  /** Output variables */
  output?: Record<string, unknown>;

  /** Error message (if failed) */
  error?: string;

  /** Execution duration in milliseconds */
  duration: number;
}

/**
 * Subgraph Context Inherit Data
 */
export interface SubgraphContextInheritData {
  /** Subgraph execution ID */
  subthreadId: string;

  /** Parent execution ID */
  parentThreadId: string;

  /** Inherited variables */
  variables: Record<string, unknown>;

  /** Inherited tools */
  tools?: string[];

  /** Inherited configuration */
  config?: Record<string, unknown>;
}

/**
 * Subgraph Context Return Data
 */
export interface SubgraphContextReturnData {
  /** Subgraph execution ID */
  subthreadId: string;

  /** Parent execution ID */
  parentThreadId: string;

  /** Output variables to return */
  output: Record<string, unknown>;

  /** Variables to set in parent */
  variables?: Record<string, unknown>;
}

/**
 * Subgraph State Sync Data
 */
export interface SubgraphStateSyncData {
  /** Subgraph execution ID */
  subthreadId: string;

  /** Parent execution ID */
  parentThreadId: string;

  /** Sync direction */
  direction: "to_parent" | "from_parent";

  /** Synced state */
  state: Record<string, unknown>;

  /** Sync reason */
  reason: "checkpoint" | "variable_change" | "explicit";
}
