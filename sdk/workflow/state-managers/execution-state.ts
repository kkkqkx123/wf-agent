/**
 * ExecutionState - Execution Status Manager
 * Manages the temporary state during the execution of workflow executions
 * Separated from persistent data, focusing solely on the management of the status during execution
 */

import type { ID } from "@wf-agent/types";
import { now } from "@wf-agent/common-utils";

/**
 * Subgraph execution context
 */
export interface SubgraphContext {
  /** Sub-workflow ID */
  workflowId: ID;
  /** Parent workflow ID */
  parentWorkflowId: ID;
  /** Start time */
  startTime: number;
  /** Input data */
  input: unknown;
  /** Current depth */
  depth: number;
}

/**
 * ExecutionState - Execution Status Manager
 *
 * Key Responsibilities:
 * - Manages the execution stack of subgraphs
 * - Provides the current workflow ID (taking into account the context of the subgraphs)
 * - Handles temporary states during execution
 *
 * Design Principles:
 * - Separated from persistent data
 * - Its lifecycle is tied to the execution cycle
 * - It is a pure state manager, without any business logic
 *
 * Note:
 * - It does not manage the execution status of triggered sub-workflows (which are handled by TaskRegistry and WorkflowExecution)
 */
export class ExecutionState {
  /**
   * Subgraph execution stack
   */
  private subgraphStack: SubgraphContext[] = [];

  /**
   * Constructor
   */
  constructor() {
    // Initialize to a blank state.
  }

  /**
   * Enter the subgraph
   * @param workflowId Sub-workflow ID
   * @param parentWorkflowId Parent-workflow ID
   * @param input Input data
   */
  enterSubgraph(workflowId: ID, parentWorkflowId: ID, input: unknown): void {
    this.subgraphStack.push({
      workflowId,
      parentWorkflowId,
      startTime: now(),
      input,
      depth: this.subgraphStack.length,
    });
  }

  /**
   * Exit the subgraph
   */
  exitSubgraph(): void {
    this.subgraphStack.pop();
  }

  /**
   * Get the current subgraph context
   * @returns The current subgraph context; returns null if not within a subgraph
   */
  getCurrentSubgraphContext(): SubgraphContext | null {
    return this.subgraphStack.length > 0
      ? this.subgraphStack[this.subgraphStack.length - 1] || null
      : null;
  }

  /**
   * Get the subgraph execution stack
   * @returns A copy of the subgraph execution stack
   */
  getSubgraphStack(): SubgraphContext[] {
    return [...this.subgraphStack];
  }

  /**
   * Check if it is executed within the subgraph.
   * @returns Whether it is within the subgraph
   */
  isInSubgraph(): boolean {
    return this.subgraphStack.length > 0;
  }

  /**
   * Get the current workflow ID (taking into account the context of the subgraph)
   * @param baseWorkflowId: The base workflow ID
   * @returns: The current workflow ID
   */
  getCurrentWorkflowId(baseWorkflowId: ID): ID {
    const context = this.getCurrentSubgraphContext();
    return context ? context.workflowId : baseWorkflowId;
  }

  /**
   * Get the current depth of the subgraph
   * @returns The current depth of the subgraph
   */
  getCurrentDepth(): number {
    return this.subgraphStack.length;
  }

  /**
   * Clean up resources
   * Clear all execution statuses
   */
  cleanup(): void {
    this.subgraphStack = [];
  }

  /**
   * Clone execution status
   * @returns A copy of the execution status
   */
  clone(): ExecutionState {
    const cloned = new ExecutionState();
    cloned.subgraphStack = this.subgraphStack.map(context => ({ ...context }));
    return cloned;
  }
}
