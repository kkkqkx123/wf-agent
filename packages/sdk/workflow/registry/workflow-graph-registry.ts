/**
 * WorkflowGraph Registry
 * Manages pre-processed workflow graphs
 *
 * This module only exports class definitions; instances are not exported. Instances are managed uniformly through the SingletonRegistry.
 *
 */

import type { ID } from "@wf-agent/types";
import type { WorkflowGraph } from "../entities/workflow-graph.js";
import { createRegistry } from "../../shared/registry/utils/registry-internals.js";
import type { MutableRegistry } from "../../shared/registry/types.js";

/**
 * WorkflowGraph Registry Class
 * Responsible for managing the pre-processed workflow graph data
 */
export class WorkflowGraphRegistry {
  private items: MutableRegistry<WorkflowGraph>;

  constructor() {
    this.items = createRegistry<WorkflowGraph>();
  }

  /**
   * Register the preprocessed workflow graph
   * @param workflowGraph The preprocessed workflow graph
   */
  register(workflowGraph: WorkflowGraph): void {
    this.items.set(workflowGraph.workflowId, workflowGraph);
  }

  /**
   * Get the preprocessed workflow graph
   * @param workflowId: Workflow ID
   * @returns: The preprocessed workflow graph; returns undefined if it does not exist
   */
  get(workflowId: ID): WorkflowGraph | undefined {
    return this.items.get(workflowId);
  }

  /**
   * Check if the workflow graph exists.
   * @param workflowId: Workflow ID
   * @returns: Whether the workflow graph exists or not
   */
  has(workflowId: ID): boolean {
    return this.items.has(workflowId);
  }

  /**
   * Remove the workflow graph
   * @param workflowId Workflow ID
   */
  unregister(workflowId: ID): void {
    this.items.delete(workflowId);
  }

  /**
   * Clear all workflow graphs.
   */
  clear(): void {
    this.items.clear();
  }

  /**
   * Get all workflow IDs
   * @returns Array of workflow IDs
   */
  getAllWorkflowIds(): ID[] {
    return this.items.keys();
  }

  /**
   * Get the number of workflow graphs
   * @returns Number of workflow graphs
   */
  size(): number {
    return this.items.size;
  }

  /**
   * Batch registration of workflow graphs
   * @param workflowGraphs: An array of preprocessed workflow graphs
   */
  registerBatch(workflowGraphs: WorkflowGraph[]): void {
    for (const workflowGraph of workflowGraphs) {
      this.register(workflowGraph);
    }
  }

  /**
   * Batch remove workflow graphs
   * @param workflowIds array of workflow IDs
   */
  unregisterBatch(workflowIds: ID[]): void {
    for (const workflowId of workflowIds) {
      this.unregister(workflowId);
    }
  }
}