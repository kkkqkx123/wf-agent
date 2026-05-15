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
import { PrometheusFormatter } from "../../../../core/metrics/utils/prometheus-formatter.js";
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
      throw new Error('Workflow metrics collector not available');
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
  async getTopWorkflows(limit: number = 10): Promise<Array<{
    workflowId: string;
    executionCount: number;
    successRate: number;
  }>> {
    const collector = this.metricsRegistry.getWorkflowCollector();
    if (!collector) {
      throw new Error('Workflow metrics collector not available');
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
  async getNodeTemplateMetrics(options?: NodeMetricsQuery): Promise<Array<{
    templateName: string;
    nodeType: string;
    instantiationCount: number;
  }>> {
    const collector = this.metricsRegistry.getNodeCollector();
    if (!collector) {
      throw new Error('Node metrics collector not available');
    }
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
    const collector = this.metricsRegistry.getNodeCollector();
    if (!collector) {
      throw new Error('Node metrics collector not available');
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
    avgToolCalls: number;
    byProfile: Record<string, number>;
  }> {
    const collector = this.metricsRegistry.getAgentCollector();
    if (!collector) {
      throw new Error('Agent metrics collector not available');
    }
    
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
    const workflowCollector = this.metricsRegistry.getWorkflowCollector();
    const nodeCollector = this.metricsRegistry.getNodeCollector();
    const agentCollector = this.metricsRegistry.getAgentCollector();
    const eventCollector = this.metricsRegistry.getEventCollector();
    
    if (!workflowCollector || !nodeCollector || !agentCollector) {
      throw new Error('One or more metrics collectors not available');
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
      throw new Error('One or more metrics collectors not available');
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
