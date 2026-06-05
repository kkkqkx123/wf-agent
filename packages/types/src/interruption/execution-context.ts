/**
 * Execution Domain Context Types
 *
 * Discriminated union of domain-specific context metadata carried by InterruptionState.
 * Each domain variant provides strongly-typed fields relevant to its execution context.
 *
 * Design:
 * - Uses a `domain` literal discriminant for type-narrowing in consumers
 * - UnknownContext provides an escape hatch for minimal/backward-compatible usage
 * - All fields are readonly to prevent accidental mutation after construction
 */

/**
 * Agent Loop Execution Context
 *
 * Identifies an agent loop execution instance with iteration tracking.
 * Agent loops are sequential by nature, so iteration + agentExecutionId is sufficient.
 */
export interface AgentLoopContext {
  readonly domain: "AGENT_LOOP";
  /** The agent loop execution instance ID */
  agentExecutionId: string;
  /** Current iteration number (0-based, starts at 0) */
  iteration: number;
}

/**
 * Workflow Node Execution Context
 *
 * Identifies a specific node execution within a workflow.
 * Even in fork scenarios, `nodeExecutionId` uniquely identifies the execution instance.
 * For fork branches, this same type is used — the `workflowId` and `nodeExecutionId`
 * provide sufficient uniqueness regardless of which branch the node belongs to.
 */
export interface WorkflowNodeContext {
  readonly domain: "WORKFLOW_NODE";
  /** The workflow definition ID */
  workflowId: string;
  /** The node template ID within the workflow graph */
  nodeId: string;
  /** Unique per node execution instance (equals the execution ID of this node's coordinator) */
  nodeExecutionId: string;
}

/**
 * Unknown/Fallback Execution Context
 *
 * Used when no domain-specific context is needed.
 * Allows arbitrary metadata via index signature for extensibility.
 */
export interface UnknownContext {
  readonly domain: "UNKNOWN";
  [key: string]: unknown;
}

/**
 * Discriminated union of all possible execution domain contexts.
 * Use the `domain` field for type narrowing:
 *
 * ```typescript
 * if (ctx.domain === "WORKFLOW_NODE") {
 *   ctx.nodeId; // typed as string
 * }
 * ```
 */
export type ExecutionDomainContext = AgentLoopContext | WorkflowNodeContext | UnknownContext;
