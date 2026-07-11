/**
 * Workflow Graph Adapter
 * Wraps WorkflowGraphQueryAPI for CLI usage
 */

import { BaseAdapter } from "./base-adapter.js";
import type {
  WorkflowGraphSummary,
  GraphNodeStats,
  GraphEdgeStats,
} from "@wf-agent/sdk/api";
import type { ID } from "@wf-agent/types";

export interface GraphQueryOptions {
  includeMetadata?: boolean;
  checkCycles?: boolean;
  analyzePaths?: boolean;
}

export class WorkflowGraphAdapter extends BaseAdapter {
  constructor() {
    super();
  }

  /**
   * Get graph summary (lightweight)
   */
  async getGraphSummary(workflowId: ID): Promise<WorkflowGraphSummary> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.getFactory().createWorkflowGraphQueryAPI();
      const summary = await api.getGraphSummary(workflowId);

      if (!summary) {
        throw new Error(`Graph not found for workflow: ${workflowId}`);
      }

      return summary;
    }, "Get graph summary");
  }

  /**
   * Get all nodes in the graph
   */
  async getNodes(workflowId: ID, nodeType?: string) {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.getFactory().createWorkflowGraphQueryAPI();

      if (nodeType) {
        const nodes = await api.getNodesByType(workflowId, nodeType);
        return nodes;
      } else {
        const nodes = await api.getNodes(workflowId);
        return nodes;
      }
    }, "Get workflow nodes");
  }

  /**
   * Get all edges in the graph
   */
  async getEdges(workflowId: ID) {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.getFactory().createWorkflowGraphQueryAPI();
      const edges = await api.getEdges(workflowId);

      return edges;
    }, "Get workflow edges");
  }

  /**
   * Analyze graph structure
   */
  async analyzeGraph(workflowId: ID): Promise<any> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.getFactory().createWorkflowGraphQueryAPI();
      const analysis = await api.getGraphAnalysis(workflowId);

      if (!analysis) {
        throw new Error(`Graph analysis not found for workflow: ${workflowId}`);
      }

      this.logOperation(`Graph analysis completed for: ${workflowId}`);
      return analysis;
    }, "Analyze workflow graph");
  }

  /**
   * Get node statistics
   */
  async getNodeStats(workflowId: ID): Promise<GraphNodeStats> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.getFactory().createWorkflowGraphQueryAPI();
      const summary = await api.getGraphSummary(workflowId);

      if (!summary || !summary.nodeStats) {
        throw new Error(`Node stats not found for workflow: ${workflowId}`);
      }

      return summary.nodeStats;
    }, "Get node statistics");
  }

  /**
   * Get edge statistics
   */
  async getEdgeStats(workflowId: ID): Promise<GraphEdgeStats> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.getFactory().createWorkflowGraphQueryAPI();
      const summary = await api.getGraphSummary(workflowId);

      if (!summary || !summary.edgeStats) {
        throw new Error(`Edge stats not found for workflow: ${workflowId}`);
      }

      return summary.edgeStats;
    }, "Get edge statistics");
  }

  /**
   * Check for cycles in graph
   */
  async hasCycles(workflowId: ID): Promise<boolean> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.getFactory().createWorkflowGraphQueryAPI();
      const analysis = await api.getGraphAnalysis(workflowId);

      if (!analysis) {
        return false;
      }

      return (analysis as any).hasCycles || false;
    }, "Check for graph cycles");
  }
}
