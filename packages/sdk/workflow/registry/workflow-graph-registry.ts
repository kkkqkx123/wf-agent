/**
 * WorkflowGraph Registry
 * Manages pre-processed workflow graphs
 *
 * This module only exports class definitions; instances are not exported. Instances are managed uniformly through the SingletonRegistry.
 *
 */

import type { ID } from "@wf-agent/types";
import type { WorkflowGraph } from "../entities/workflow-graph.js";
import { RegistryImpl } from "../../shared/registry/utils/registry-internals.js";

/**
 * WorkflowGraph Registry Class
 * Responsible for managing the pre-processed workflow graph data
 *
 * Extends RegistryImpl<WorkflowGraph> for base CRUD operations.
 */
export class WorkflowGraphRegistry extends RegistryImpl<WorkflowGraph> {
  /**
   * Register the preprocessed workflow graph
   * @param workflowGraph The preprocessed workflow graph
   */
  register(workflowGraph: WorkflowGraph): void {
    this.set(workflowGraph.workflowId, workflowGraph);
  }

  /**
   * Get the preprocessed workflow graph
   * @param workflowId: Workflow ID
   * @returns: The preprocessed workflow graph; returns undefined if it does not exist
   */
  override get(workflowId: ID): WorkflowGraph | undefined {
    return this.items.get(workflowId);
  }

  /**
   * Remove the workflow graph
   * @param workflowId Workflow ID
   */
  unregister(workflowId: ID): void {
    this.delete(workflowId);
  }

  /**
   * Get all workflow IDs
   * @returns Array of workflow IDs
   */
  getAllWorkflowIds(): ID[] {
    return this.keys();
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