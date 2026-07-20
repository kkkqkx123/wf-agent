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

  async exportMetrics(format: string, outputFile?: string): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("exportMetrics", { format, outputFile });
      const report = await this.getComprehensiveReport();
      let content: string;
      if (format === "json") {
        content = JSON.stringify(report, null, 2);
      } else if (format === "prometheus") {
        const lines: string[] = [];
        lines.push("# HELP wf_agent_workflow_executions_total Total workflow executions");
        lines.push("# TYPE wf_agent_workflow_executions_total counter");
        const wfMetrics = report["workflowMetrics"] as Record<string, any> | undefined;
        if (wfMetrics) {
          lines.push(`wf_agent_workflow_executions_total ${wfMetrics["totalExecutions"] || 0}`);
          lines.push(`wf_agent_workflow_success_rate ${wfMetrics["successRate"] || 0}`);
          lines.push(`wf_agent_workflow_avg_duration_ms ${wfMetrics["avgDuration"] || 0}`);
        }
        const agentMetrics = report["agentMetrics"] as Record<string, any> | undefined;
        if (agentMetrics) {
          lines.push(`wf_agent_agent_executions_total ${agentMetrics["totalExecutions"] || 0}`);
          lines.push(`wf_agent_agent_avg_iterations ${agentMetrics["avgIterations"] || 0}`);
        }
        content = lines.join("\n");
      } else {
        content = JSON.stringify(report, null, 2);
      }
      return { format, outputFile, content };
    }, "Export metrics");
  }
}