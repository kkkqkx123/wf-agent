/**
 * Metrics Adapter
 * Query and export metrics data.
 */

import { BaseAdapter } from "./base-adapter.js";

export class MetricsAdapter extends BaseAdapter {
  override getResourceName(): string {
    return "Metrics";
  }

  async getWorkflowMetrics(options?: { workflowId?: string }): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("getWorkflowMetrics", options);
      const api = this.sdk.metrics;
      const result = await api.getWorkflowMetrics(options as any);
      return result as any;
    }, "Get workflow metrics");
  }

  async getNodeTemplateMetrics(options?: { topN?: number }): Promise<Record<string, any>[]> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("getNodeTemplateMetrics", options);
      const api = this.sdk.metrics;
      const result = await api.getNodeTemplateMetrics(options as any);
      return result as any;
    }, "Get node template metrics");
  }

  async getAgentMetrics(options?: { profileId?: string }): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("getAgentMetrics", options);
      const api = this.sdk.metrics;
      const result = await api.getAgentMetrics(options as any);
      return result as any;
    }, "Get agent metrics");
  }

  async getComprehensiveReport(): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("getComprehensiveReport");
      const api = this.sdk.metrics;
      // MetricsResourceAPI doesn't have getComprehensiveReport, compose from available methods
      const [workflow, agents, nodeTemplates] = await Promise.all([
        api.getWorkflowMetrics({} as any).catch(() => null),
        api.getAgentMetrics({} as any).catch(() => null),
        api.getNodeTemplateMetrics({} as any).catch(() => null),
      ]);
      return {
        timestamp: Date.now(),
        summary: {
          totalMetrics: 3,
          byType: {
            workflow: workflow ? 1 : 0,
            agents: agents ? 1 : 0,
            nodeTemplates: nodeTemplates ? 1 : 0,
          },
        },
        workflowMetrics: workflow,
        agentMetrics: agents,
        nodeTemplateMetrics: nodeTemplates,
      };
    }, "Get comprehensive report");
  }
}