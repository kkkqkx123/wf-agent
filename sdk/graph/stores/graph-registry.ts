/**
 * Graph Registry
 * Manages pre-processed graphs
 *
 * This module only exports class definitions; instances are not exported. Instances are managed uniformly through the SingletonRegistry.
 *
 */

import type { PreprocessedGraph, ID } from "@wf-agent/types";

/**
 * Graph Registry Class
 * Responsible for managing the pre-processed graph data
 */
export class GraphRegistry {
  private graphs: Map<ID, PreprocessedGraph> = new Map();

  /**
   * Register the preprocessed graph
   * @param graph The preprocessed graph
   */
  register(graph: PreprocessedGraph): void {
    this.graphs.set(graph.workflowId, graph);
  }

  /**
   * Get the preprocessed image
   * @param workflowId: Workflow ID
   * @returns: The preprocessed image; returns undefined if it does not exist
   */
  get(workflowId: ID): PreprocessedGraph | undefined {
    return this.graphs.get(workflowId);
  }

  /**
   * Check if the image exists.
   * @param workflowId: Workflow ID
   * @returns: Whether the image exists or not
   */
  has(workflowId: ID): boolean {
    return this.graphs.has(workflowId);
  }

  /**
   * Remove the image
   * @param workflowId Workflow ID
   */
  unregister(workflowId: ID): void {
    this.graphs.delete(workflowId);
  }

  /**
   * Clear all images.
   */
  clear(): void {
    this.graphs.clear();
  }

  /**
   * Get all workflow IDs
   * @returns Array of workflow IDs
   */
  getAllWorkflowIds(): ID[] {
    return Array.from(this.graphs.keys());
  }

  /**
   * Get the number of images
   * @returns Number of images
   */
  size(): number {
    return this.graphs.size;
  }

  /**
   * Batch registration of graphs
   * @param graphs: An array of preprocessed graphs
   */
  registerBatch(graphs: PreprocessedGraph[]): void {
    for (const graph of graphs) {
      this.register(graph);
    }
  }

  /**
   * Batch remove images
   * @param workflowIds array of workflow IDs
   */
  unregisterBatch(workflowIds: ID[]): void {
    for (const workflowId of workflowIds) {
      this.unregister(workflowId);
    }
  }
}
