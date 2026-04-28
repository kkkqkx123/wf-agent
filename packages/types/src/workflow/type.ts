/**
 * Workflow Enumeration Type Definition
 */

/**
 * Workflow Template Types
 * Used to differentiate between different types of workflows, affecting preprocessing timing and checkpointing strategies
 */
export type WorkflowTemplateType =
  /** Trigger subworkflow: must contain START_FROM_TRIGGER and CONTINUE_FROM_TRIGGER nodes, not start, end, subgraph nodes */
  | "TRIGGERED_SUBWORKFLOW"
  /** Standalone workflow: does not contain EXECUTE_TRIGGERED_SUBGRAPH triggers and does not contain SUBGRAPH nodes */
  | "STANDALONE"
  /** Dependent workflow: contains EXECUTE_TRIGGERED_SUBGRAPH trigger or SUBGRAPH node */
  | "DEPENDENT";
