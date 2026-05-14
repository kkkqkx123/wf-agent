/**
 * MetricsResourceAPI - Metrics Resource Management API
 * 
 * Provides read-only access to metrics data collected during execution.
 * Supports querying workflow, node, agent, and event metrics.
 * 
 * Design Principles:
 * - Read-only API (metrics are collected automatically, not manually created)
 * - Provides aggregated statistics and query capabilities
 * - Supports filtering by various dimensions (workflow ID, node type, profile, etc.)
 * - Exports metrics in multiple formats (JSON, Prometheus)
 */

import type { APIDependencyManager } from "../../core/sdk-dependencies.js";
import type { MetricsRegistry } from "../../../../core/metrics/metrics-registry.js";
import type { MetricReport } from "../../../../core/metrics/types.js";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "MetricsResourceAPI" });

/**
 * Workflow metrics query options
 */
export interface WorkflowMetricsQuery {
  /** Filter by workflow ID */
  workflowId?: string;
  /** Get top N workflows by execution count */
  topN?: number;
}

/**
 * Node metrics query options
 */
export interface NodeMetricsQuery {
  /** Filter by node type */
  nodeType?: string;
  /** Get top N templates by instantiation count */
  topN?: number;
}

/**
 * Agent metrics query options
 */
export interface AgentMetricsQuery {
  /** Filter by profile ID */
  profileId?: string;
}

/**
 * Export format options
 */
export type MetricsExportFormat = "json" | "prometheus";

/**
 * Metrics Resource API
 * Provides access to all metrics collectors and their data
 * 
 * Note: This is not a traditional resource API, but a specialized metrics query API.
 * It doesn't follow the typical CRUD pattern since metrics are collected automatically.
 */
export class MetricsResourceAPI {
  private metricsRegistry: MetricsRegistry;

  constructor(dependencies: APIDependencyManager) {
    this.metricsRegistry = dependencies.getGlobalContext().metricsRegistry;
    logger.info("MetricsResourceAPI initialized");
  }

  // ============================================================
  // Workflow Metrics
  // ============================================================

  /**
   * Get workflow execution metrics
   * @param options Query options
   * @returns Workflow metrics statistics
   */
  async getWorkflowMetrics(options?: WorkflowMetricsQuery): Promise<{
    totalExecutions: number;
    successRate: number;
    avgDuration: number;
    p95Duration: number;
    p99Duration: number;
    byVersion: Record<string, number>;
  }> {
    const collector = this.metricsRegistry.getCollectors().workflow;
    
    if (options?.workflowId) {
      return collector.getWorkflowUsageStats(options.workflowId);
    }
    
    // Return overall stats if no specific workflow ID
    return collector.getWorkflowUsageStats();
  }

  /**
   * Get top workflows by execution count
   * @param limit Maximum number of workflows to return (default: 10)
   * @returns Array of workflow statistics sorted by execution count
   */
  async getTopWorkflows(limit: number = 10): Promise<Array<{
    workflowId: string;
    executionCount: number;
    successRate: number;
  }>> {
    const collector = this.metricsRegistry.getCollectors().workflow;
    return collector.getTopWorkflows(limit);
  }

  // ============================================================
  // Node Metrics
  // ============================================================

  /**
   * Get node template usage metrics
   * @param options Query options
   * @returns Node template statistics
   */
  async getNodeTemplateMetrics(options?: NodeMetricsQuery): Promise<Array<{
    templateName: string;
    nodeType: string;
    instantiationCount: number;
  }>> {
    const collector = this.metricsRegistry.getCollectors().node;
    const limit = options?.topN || 10;
    return collector.getTopNodeTemplates(limit);
  }

  /**
   * Get node execution statistics by type
   * @returns Statistics grouped by node type
   */
  async getNodeExecutionStatsByType(): Promise<Record<string, {
    totalCount: number;
    successRate: number;
    avgDuration: number;
  }>> {
    const collector = this.metricsRegistry.getCollectors().node;
    return collector.getNodeExecutionStatsByType();
  }

  // ============================================================
  // Agent Metrics
  // ============================================================

  /**
   * Get agent loop execution metrics
   * @param options Query options
   * @returns Agent loop statistics
   */
  async getAgentMetrics(options?: AgentMetricsQuery): Promise<{
    totalExecutions: number;
    avgIterations: number;
    avgToolCalls: number;
    byProfile: Record<string, number>;
  }> {
    const collector = this.metricsRegistry.getCollectors().agent;
    
    if (options?.profileId) {
      return collector.getAgentStats(options.profileId);
    }
    
    // Return overall stats if no specific profile ID
    return collector.getAgentStats();
  }

  // ============================================================
  // Comprehensive Reports
  // ============================================================

  /**
   * Generate comprehensive metrics report
   * @param options Report generation options
   * @returns Comprehensive metrics report
   */
  async generateReport(options?: {
    timeRange?: { from: number; to: number };
    includeTrends?: boolean;
  }): Promise<MetricReport> {
    logger.debug("Generating comprehensive metrics report", { options });
    return await this.metricsRegistry.generateReport(options);
  }

  /**
   * Subscribe to periodic metric reports
   * @param callback Callback function to receive reports
   * @returns Unsubscribe function
   */
  onReport(callback: (report: MetricReport) => void | Promise<void>): () => void {
    return this.metricsRegistry.onReport(callback);
  }

  // ============================================================
  // Export Functions
  // ============================================================

  /**
   * Export metrics in specified format
   * @param format Export format (json or prometheus)
   * @returns Formatted metrics string
   */
  async exportMetrics(format: MetricsExportFormat): Promise<string> {
    logger.debug("Exporting metrics", { format });
    
    switch (format) {
      case "json":
        return await this.exportAsJSON();
      case "prometheus":
        return await this.exportAsPrometheus();
      default:
        throw new Error(`Unsupported export format: ${format}`);
    }
  }

  /**
   * Export metrics as JSON
   * @returns JSON formatted metrics
   */
  private async exportAsJSON(): Promise<string> {
    const report = await this.generateReport();
    return JSON.stringify(report, null, 2);
  }

  /**
   * Export metrics in Prometheus format
   * @returns Prometheus formatted metrics
   */
  private async exportAsPrometheus(): Promise<string> {
    const collectors = this.metricsRegistry.getCollectors();
    const lines: string[] = [];
    const timestamp = Date.now();

    // Helper function to format labels
    const formatLabels = (labels: Record<string, string>): string => {
      const parts = Object.entries(labels)
        .map(([key, value]) => `${key}="${value}"`)
        .join(",");
      return parts ? `{${parts}}` : "";
    };

    // Export workflow metrics
    const workflowStats = collectors.workflow.getWorkflowUsageStats();
    lines.push(`# HELP workflow_execution_total Total workflow executions`);
    lines.push(`# TYPE workflow_execution_total counter`);
    lines.push(`workflow_execution_total ${workflowStats.totalExecutions}`);
    
    lines.push(`# HELP workflow_execution_success_rate Workflow execution success rate`);
    lines.push(`# TYPE workflow_execution_success_rate gauge`);
    lines.push(`workflow_execution_success_rate ${workflowStats.successRate}`);
    
    lines.push(`# HELP workflow_execution_duration_seconds Workflow execution duration in seconds`);
    lines.push(`# TYPE workflow_execution_duration_seconds histogram`);
    lines.push(`workflow_execution_duration_seconds_avg ${workflowStats.avgDuration / 1000}`);
    lines.push(`workflow_execution_duration_seconds_p95 ${workflowStats.p95Duration / 1000}`);
    lines.push(`workflow_execution_duration_seconds_p99 ${workflowStats.p99Duration / 1000}`);

    // Export node metrics
    const nodeStats = collectors.node.getNodeExecutionStatsByType();
    lines.push(`# HELP node_execution_total Total node executions by type`);
    lines.push(`# TYPE node_execution_total counter`);
    for (const [nodeType, stats] of Object.entries(nodeStats)) {
      lines.push(`node_execution_total{node_type="${nodeType}"} ${stats.totalCount}`);
    }

    // Export agent metrics
    const agentStats = collectors.agent.getAgentStats();
    lines.push(`# HELP agent_loop_execution_total Total agent loop executions`);
    lines.push(`# TYPE agent_loop_execution_total counter`);
    lines.push(`agent_loop_execution_total ${agentStats.totalExecutions}`);
    
    lines.push(`# HELP agent_loop_iterations_avg Average iterations per agent loop`);
    lines.push(`# TYPE agent_loop_iterations_avg gauge`);
    lines.push(`agent_loop_iterations_avg ${agentStats.avgIterations}`);

    // Add timestamp comment
    lines.push(`# Generated at ${new Date(timestamp).toISOString()}`);

    return lines.join("\n") + "\n";
  }

  // ============================================================
  // Flush and Cleanup
  // ============================================================

  /**
   * Flush all metrics (write to storage if configured)
   */
  async flush(): Promise<void> {
    logger.debug("Flushing all metrics");
    await this.metricsRegistry.flushAll();
  }
}
