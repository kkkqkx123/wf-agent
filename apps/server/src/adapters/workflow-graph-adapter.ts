/**
 * Workflow Graph Adapter
 * Query and analyze workflow graph structure.
 */

import { BaseAdapter } from "./base-adapter.js";
import { findByIdOrThrow } from "@wf-agent/runtime/adapters";
import type { ID } from "@wf-agent/types";
import type {
  WorkflowGraphSummary,
  GraphNodeStats,
  GraphEdgeStats,
} from "@wf-agent/sdk/api";

export class WorkflowGraphAdapter extends BaseAdapter {
  override getResourceName(): string {
    return "WorkflowGraph";
  }

  private getGraphAPI() {
    return this.sdk.getFactory().createWorkflowGraphQueryAPI();
  }

  /**
   * Get graph summary (lightweight)
   */
  async getGraphSummary(workflowId: ID): Promise<WorkflowGraphSummary> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("getGraphSummary", { workflowId });
      await findByIdOrThrow(this.sdk.workflows, workflowId as string, "Workflow");
      const api = this.getGraphAPI();
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
      this.logOperation("getNodes", { workflowId, nodeType });
      await findByIdOrThrow(this.sdk.workflows, workflowId as string, "Workflow");
      const api = this.getGraphAPI();
      if (nodeType) {
        return await api.getNodesByType(workflowId, nodeType);
      }
      return await api.getNodes(workflowId);
    }, "Get workflow nodes");
  }

  /**
   * Get all edges in the graph
   */
  async getEdges(workflowId: ID) {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("getEdges", { workflowId });
      await findByIdOrThrow(this.sdk.workflows, workflowId as string, "Workflow");
      const api = this.getGraphAPI();
      return await api.getEdges(workflowId);
    }, "Get workflow edges");
  }

  /**
   * Analyze graph structure
   */
  async analyzeGraph(workflowId: ID): Promise<any> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("analyzeGraph", { workflowId });
      await findByIdOrThrow(this.sdk.workflows, workflowId as string, "Workflow");
      const api = this.getGraphAPI();
      const analysis = await api.getGraphAnalysis(workflowId);
      if (!analysis) {
        throw new Error(`Graph analysis not found for workflow: ${workflowId}`);
      }
      return analysis;
    }, "Analyze workflow graph");
  }

  /**
   * Get node statistics
   */
  async getNodeStats(workflowId: ID): Promise<GraphNodeStats> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("getNodeStats", { workflowId });
      await findByIdOrThrow(this.sdk.workflows, workflowId as string, "Workflow");
      const api = this.getGraphAPI();
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
      this.logOperation("getEdgeStats", { workflowId });
      await findByIdOrThrow(this.sdk.workflows, workflowId as string, "Workflow");
      const api = this.getGraphAPI();
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
      this.logOperation("hasCycles", { workflowId });
      await findByIdOrThrow(this.sdk.workflows, workflowId as string, "Workflow");
      const api = this.getGraphAPI();
      const analysis = await api.getGraphAnalysis(workflowId);
      return analysis ? ((analysis as any).hasCycles || false) : false;
    }, "Check for graph cycles");
  }
}