/**
 * Metrics Adapter
 * Wraps metrics-related SDK API calls for CLI usage
 */

import { BaseAdapter } from "./base-adapter.js";
import type {
  WorkflowMetricsQuery,
  NodeMetricsQuery,
  AgentMetricsQuery,
  MetricsExportFormat,
} from "@wf-agent/sdk/api";

/**
 * Metrics Adapter
 */
export class MetricsAdapter extends BaseAdapter {
  constructor() {
    super();
  }

  /**
   * Get workflow execution metrics
   */
  async getWorkflowMetrics(options?: {
    workflowId?: string;
  }) {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.metrics;

      // Build query options
      const query: WorkflowMetricsQuery | undefined = options ? {
        workflowId: options.workflowId,
      } : undefined;

      const metrics = await api.getWorkflowMetrics(query);
      return metrics;
    }, "Get workflow metrics");
  }

  /**
   * Get node template metrics
   */
  async getNodeTemplateMetrics(options?: {
    topN?: number;
  }) {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.metrics;

      // Build query options
      const query: NodeMetricsQuery | undefined = options ? {
        topN: options.topN,
      } : undefined;

      const metrics = await api.getNodeTemplateMetrics(query);
      return metrics;
    }, "Get node template metrics");
  }

  /**
   * Get agent loop metrics
   */
  async getAgentMetrics(options?: {
    profileId?: string;
  }) {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.metrics;

      // Build query options
      const query: AgentMetricsQuery | undefined = options ? {
        profileId: options.profileId,
      } : undefined;

      const metrics = await api.getAgentMetrics(query);
      return metrics;
    }, "Get agent metrics");
  }

  /**
   * Get comprehensive metrics report
   */
  async getComprehensiveReport() {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.metrics;

      const report = await api.generateReport();

      return report;
    }, "Get comprehensive metrics report");
  }

  /**
   * Export metrics to file
   */
  async exportMetrics(format: MetricsExportFormat, outputFile?: string) {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.metrics;

      const content = await api.exportMetrics(format);

      if (outputFile) {
        // Write to file
        const { writeFile } = await import("fs/promises");
        const { resolve } = await import("path");
        const fullPath = resolve(process.cwd(), outputFile);
        await writeFile(fullPath, content, "utf-8");
        return { outputFile: fullPath, format };
      }

      return { content, format };
    }, "Export metrics");
  }

  /**
   * Subscribe to periodic metrics reports
   */
  subscribeToReports(
    callback: (report: any) => void | Promise<void>,
  ) {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.metrics;

      const unsubscribe = api.onReport(callback);

      return unsubscribe;
    }, "Subscribe to metrics reports");
  }
}
