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
import type { MetricsRegistry } from "../../../../metrics/metrics-registry.js";
import type { MetricReport } from "../../../../metrics/types.js";
import { PrometheusFormatter } from "../../../../metrics/utils/prometheus-formatter.js";
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
    const collector = this.metricsRegistry.getWorkflowCollector();
    if (!collector) {
      throw new Error("Workflow metrics collector not available");
    }

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
  async getTopWorkflows(limit: number = 10): Promise<
    Array<{
      workflowId: string;
      executionCount: number;
      successRate: number;
    }>
  > {
    const collector = this.metricsRegistry.getWorkflowCollector();
    if (!collector) {
      throw new Error("Workflow metrics collector not available");
    }
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
  async getNodeTemplateMetrics(options?: NodeMetricsQuery): Promise<
    Array<{
      templateName: string;
      nodeType: string;
      instantiationCount: number;
    }>
  > {
    const collector = this.metricsRegistry.getNodeCollector();
    if (!collector) {
      throw new Error("Node metrics collector not available");
    }
    const limit = options?.topN || 10;
    return collector.getTopNodeTemplates(limit);
  }

  /**
   * Get node execution statistics by type
   * @returns Statistics grouped by node type
   */
  async getNodeExecutionStatsByType(): Promise<
    Record<
      string,
      {
        totalCount: number;
        successRate: number;
        avgDuration: number;
      }
    >
  > {
    const collector = this.metricsRegistry.getNodeCollector();
    if (!collector) {
      throw new Error("Node metrics collector not available");
    }
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
    byProfile: Record<string, number>;
  }> {
    const collector = this.metricsRegistry.getAgentCollector();
    if (!collector) {
      throw new Error("Agent metrics collector not available");
    }

    if (options?.profileId) {
      return collector.getAgentStats(options.profileId);
    }

    // Return overall stats if no specific profile ID
    return collector.getAgentStats();
  }

  /**
   * Get agent tool usage metrics
   *
   * Returns tool call frequency and duration statistics for agent executions.
   *
   * @param agentLoopId Optional agent loop ID to filter by (if supported by collector)
   * @returns Tool metrics including call count and total duration by tool name
   */
  async getAgentToolMetrics(agentLoopId?: string): Promise<{
    byTool: Record<string, { callCount: number; totalDuration: number; avgDuration: number }>;
    totalToolCalls: number;
  }> {
    const collector = this.metricsRegistry.getAgentCollector();
    if (!collector) {
      throw new Error("Agent metrics collector not available");
    }

    const filter = agentLoopId
      ? { labels: { agent_loop_id: agentLoopId } }
      : {};

    // Query tool call count
    const toolCountResult = collector.query({
      metricName: "agent.tool.call.count",
      metricType: "counter",
      ...filter,
    });

    // Query tool execution duration
    const toolDurationResult = collector.query({
      metricName: "agent.tool.execution.duration",
      metricType: "histogram",
      ...filter,
    });

    const byTool: Record<string, { callCount: number; totalDuration: number; avgDuration: number }> = {};
    let totalToolCalls = 0;

    // Aggregate by tool name
    const countMetric = toolCountResult.metrics.get("agent.tool.call.count");
    if (countMetric) {
      for (const [labelKey, labelAgg] of countMetric.byLabel.entries()) {
        try {
          const labels = JSON.parse(labelKey);
          const toolName = labels.tool_name;
          if (toolName) {
            byTool[toolName] = byTool[toolName] || { callCount: 0, totalDuration: 0, avgDuration: 0 };
            byTool[toolName].callCount += labelAgg.value;
            totalToolCalls += labelAgg.value;
          }
        } catch {
          // Skip malformed label keys
        }
      }
    }

    // Merge duration data
    const durationMetric = toolDurationResult.metrics.get("agent.tool.execution.duration");
    if (durationMetric) {
      for (const [labelKey, labelAgg] of durationMetric.byLabel.entries()) {
        try {
          const labels = JSON.parse(labelKey);
          const toolName = labels.tool_name;
          if (toolName && byTool[toolName]) {
            byTool[toolName].totalDuration += labelAgg.value;
            byTool[toolName].avgDuration =
              byTool[toolName].callCount > 0
                ? byTool[toolName].totalDuration / byTool[toolName].callCount
                : 0;
          }
        } catch {
          // Skip malformed label keys
        }
      }
    }

    return { byTool, totalToolCalls };
  }

  /**
   * Get agent LLM usage metrics
   *
   * Returns token consumption and cost statistics for agent LLM invocations.
   *
   * @param agentLoopId Optional agent loop ID to filter by (if supported by collector)
   * @returns LLM metrics including token usage and cost
   */
  async getAgentLLMMetrics(agentLoopId?: string): Promise<{
    totalTokens: number;
    avgTokensPerIteration: number;
    totalCostUsd: number;
    byModel: Record<string, { tokens: number; costUsd: number }>;
  }> {
    const collector = this.metricsRegistry.getAgentCollector();
    if (!collector) {
      throw new Error("Agent metrics collector not available");
    }

    const filter = agentLoopId
      ? { labels: { agent_loop_id: agentLoopId } }
      : {};

    // Query tokens per iteration
    const tokenResult = collector.query({
      metricName: "agent.loop.tokens_per_iteration",
      metricType: "histogram",
      ...filter,
    });

    // Query iteration count
    const iterationResult = collector.query({
      metricName: "agent.loop.iteration.count",
      metricType: "counter",
      ...filter,
    });

    const byModel: Record<string, { tokens: number; costUsd: number }> = {};
    let totalTokens = 0;
    let totalCostUsd = 0;

    const tokenMetric = tokenResult.metrics.get("agent.loop.tokens_per_iteration");
    if (tokenMetric) {
      totalTokens = tokenMetric.value;
      // Model-level breakdown would require additional labeling in the collector
      // For now, aggregate under "unknown"
      byModel["unknown"] = {
        tokens: totalTokens,
        costUsd: 0,
      };
    }

    const iterationMetric = iterationResult.metrics.get("agent.loop.iteration.count");
    const totalIterations = iterationMetric ? iterationMetric.value : 0;

    return {
      totalTokens,
      avgTokensPerIteration: totalIterations > 0 ? totalTokens / totalIterations : 0,
      totalCostUsd,
      byModel,
    };
  }

  /**
   * Get agent error metrics
   *
   * Returns error frequency statistics for agent executions.
   *
   * @param agentLoopId Optional agent loop ID to filter by (if supported by collector)
   * @returns Error metrics including error frequency by type
   */
  async getAgentErrorMetrics(agentLoopId?: string): Promise<{
    totalErrors: number;
    byErrorType: Record<string, number>;
  }> {
    const collector = this.metricsRegistry.getAgentCollector();
    if (!collector) {
      throw new Error("Agent metrics collector not available");
    }

    const filter = agentLoopId
      ? { labels: { agent_loop_id: agentLoopId } }
      : {};

    // Query agent error metrics (if available from collector)
    const errorResult = collector.query({
      metricName: "agent.error.count",
      metricType: "counter",
      ...filter,
    });

    const byErrorType: Record<string, number> = {};
    let totalErrors = 0;

    const errorMetric = errorResult.metrics.get("agent.error.count");
    if (errorMetric) {
      totalErrors = errorMetric.value;
      for (const [labelKey, labelAgg] of errorMetric.byLabel.entries()) {
        try {
          const labels = JSON.parse(labelKey);
          const errorType = labels.error_type || "unknown";
          byErrorType[errorType] = (byErrorType[errorType] || 0) + labelAgg.value;
        } catch {
          byErrorType["unknown"] = (byErrorType["unknown"] || 0) + labelAgg.value;
        }
      }
    }

    return { totalErrors, byErrorType };
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
    const workflowCollector = this.metricsRegistry.getWorkflowCollector();
    const nodeCollector = this.metricsRegistry.getNodeCollector();
    const agentCollector = this.metricsRegistry.getAgentCollector();
    const eventCollector = this.metricsRegistry.getEventCollector();

    if (!workflowCollector || !nodeCollector || !agentCollector) {
      throw new Error("One or more metrics collectors not available");
    }

    const result = {
      timestamp: Date.now(),
      workflow: workflowCollector.toJSON(),
      node: nodeCollector.toJSON(),
      agent: agentCollector.toJSON(),
      event: eventCollector?.toJSON() || null,
    };

    return JSON.stringify(result, null, 2);
  }

  /**
   * Export metrics in Prometheus format
   *
   * This is now MUCH simpler - just delegate to each collector!
   */
  private async exportAsPrometheus(): Promise<string> {
    const workflowCollector = this.metricsRegistry.getWorkflowCollector();
    const nodeCollector = this.metricsRegistry.getNodeCollector();
    const agentCollector = this.metricsRegistry.getAgentCollector();
    const eventCollector = this.metricsRegistry.getEventCollector();

    if (!workflowCollector || !nodeCollector || !agentCollector) {
      throw new Error("One or more metrics collectors not available");
    }

    // Each collector exports its own metrics
    const allMetrics: string[][] = [
      workflowCollector.toPrometheus(),
      nodeCollector.toPrometheus(),
      agentCollector.toPrometheus(),
    ];

    // Add event metrics if available
    if (eventCollector) {
      try {
        allMetrics.push(eventCollector.toPrometheus());
      } catch (error) {
        logger.warn("Failed to export event metrics", { error });
      }
    }

    // Combine all metrics with proper formatting
    return PrometheusFormatter.combine(allMetrics);
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
