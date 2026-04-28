/**
 * Workflow reference checking related type definitions
 */

/**
 * Workflow reference information
 */
export interface WorkflowReference {
  /** reference type */
  type: "subgraph" | "trigger" | "thread";
  /** Reference source ID (parent workflow ID, trigger ID, thread ID, etc.) */
  sourceId: string;
  /** Cite source name */
  sourceName: string;
  /** Whether it is a runtime reference (active thread or trigger) */
  isRuntimeReference: boolean;
  /** Citation details */
  details: Record<string, unknown>;
}

/**
 * Workflow reference check results
 */
export interface WorkflowReferenceInfo {
  /** Whether a reference exists */
  hasReferences: boolean;
  /** List of all references */
  references: WorkflowReference[];
  /** Can it be safely deleted (no runtime references) */
  canSafelyDelete: boolean;
  /** Citation statistics */
  stats: {
    subgraphReferences: number;
    triggerReferences: number;
    threadReferences: number;
    runtimeReferences: number;
  };
}

/**
 * Workflow reference types
 */
export type WorkflowReferenceType = "subgraph" | "trigger" | "thread";

/**
 * Workflow referencing relationships
 */
export interface WorkflowReferenceRelation {
  /** Source workflow ID */
  sourceWorkflowId: string;
  /** Target Workflow ID */
  targetWorkflowId: string;
  /** reference type */
  referenceType: WorkflowReferenceType;
  /** Whether it is a runtime reference */
  isRuntime: boolean;
  /** Referencing source IDs (thread IDs, trigger IDs, etc.) */
  sourceReferenceId?: string;
  /** Citation details */
  details?: Record<string, unknown>;
}
