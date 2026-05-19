/**
 * Metrics Registry
 * 
 * Manages all metrics collectors and provides centralized access.
 * 
 * Design Principles:
 * - Unified management of all metric collectors
 * - Centralized configuration and lifecycle management
 * - Aggregated reporting across all collectors
 * - Decoupled from EventRegistry (creates its own EventCollector)
 */

import { WorkflowMetricsCollector } from "./workflow-collector.js";
import { NodeMetricsCollector } from "./node-collector.js";
import { AgentMetricsCollector } from "./agent-collector.js";
import { EventMetricsCollector } from "./event-collector.js";
import { ToolMetricsCollector } from "./tool-collector.js";
import { TokenMetricsCollector } from "./token-collector.js";
import { TemplateMetricsCollector } from "./template-collector.js";
import { ConfigMetricsCollector } from "./config-collector.js";
import { ErrorMetricsCollector } from "./error-collector.js";
import { ResourceMetricsCollector } from "./resource-collector.js";
import { AgentLoopMetricsCollector } from "./agent-loop-collector.js";
import { TimeoutMetricsCollector } from "./timeout-collector.js";
import type { MetricCollectorConfig, MetricReport, MetricType } from "./types.js";
import { BaseMetricCollector } from "./base-collector.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "MetricsRegistry" });

/**
 * Collector name type for type-safe access
 */
type CollectorName = 
  | 'workflow' 
  | 'node' 
  | 'agent' 
  | 'event' 
  | 'tool' 
  | 'token' 
  | 'template' 
  | 'config' 
  | 'error' 
  | 'resource' 
  | 'agentLoop'
  | 'timeout';

/**
 * Type mapping from collector name to collector type
 */
type CollectorTypeMap = {
  workflow: WorkflowMetricsCollector;
  node: NodeMetricsCollector;
  agent: AgentMetricsCollector;
  event: EventMetricsCollector;
  tool: ToolMetricsCollector;
  token: TokenMetricsCollector;
  template: TemplateMetricsCollector;
  config: ConfigMetricsCollector;
  error: ErrorMetricsCollector;
  resource: ResourceMetricsCollector;
  agentLoop: AgentLoopMetricsCollector;
  timeout: TimeoutMetricsCollector;
};

export interface MetricsRegistryConfig {
  workflowMetrics?: MetricCollectorConfig;
  nodeMetrics?: MetricCollectorConfig;
  agentMetrics?: MetricCollectorConfig;
  eventMetrics?: MetricCollectorConfig;
  toolMetrics?: MetricCollectorConfig;
  tokenMetrics?: MetricCollectorConfig;
  templateMetrics?: MetricCollectorConfig;
  configMetrics?: MetricCollectorConfig;
  errorMetrics?: MetricCollectorConfig;
  resourceMetrics?: MetricCollectorConfig;
  agentLoopMetrics?: MetricCollectorConfig;
  timeoutMetrics?: MetricCollectorConfig;
  enablePeriodicReporting?: boolean;
  reportingInterval?: number;
}

export class MetricsRegistry {
  private collectors: Map<string, BaseMetricCollector>;
  private reportSubscribers: Array<(report: MetricReport) => void | Promise<void>> = [];
  private reportingTimer: NodeJS.Timeout | null = null;
  private isStarted: boolean = false;

  constructor(config?: MetricsRegistryConfig) {
    this.collectors = new Map();
    
    // Initialize all collectors with their respective configs
    this.registerCollector('workflow', new WorkflowMetricsCollector(config?.workflowMetrics));
    this.registerCollector('node', new NodeMetricsCollector(config?.nodeMetrics));
    this.registerCollector('agent', new AgentMetricsCollector(config?.agentMetrics));
    this.registerCollector('event', new EventMetricsCollector(config?.eventMetrics));
    this.registerCollector('tool', new ToolMetricsCollector(config?.toolMetrics));
    this.registerCollector('token', new TokenMetricsCollector(config?.tokenMetrics));
    this.registerCollector('template', new TemplateMetricsCollector(config?.templateMetrics));
    this.registerCollector('config', new ConfigMetricsCollector(config?.configMetrics));
    this.registerCollector('error', new ErrorMetricsCollector(config?.errorMetrics));
    this.registerCollector('resource', new ResourceMetricsCollector(config?.resourceMetrics));
    this.registerCollector('agentLoop', new AgentLoopMetricsCollector(config?.agentLoopMetrics));
    this.registerCollector('timeout', new TimeoutMetricsCollector(config?.timeoutMetrics));

    // Setup periodic reporting if enabled
    if (config?.enablePeriodicReporting) {
      this.setupPeriodicReporting(config.reportingInterval || 60000); // Default: 1 minute
    }

    logger.info("Unified Metrics Manager initialized", { 
      collectorCount: this.collectors.size 
    });
  }

  /**
   * Register a collector
   */
  private registerCollector(name: string, collector: BaseMetricCollector): void {
    this.collectors.set(name, collector);
    logger.debug(`Registered collector: ${name}`);
  }

  /**
   * Get all collectors
   */
  getCollectors(): Map<string, BaseMetricCollector> {
    return new Map(this.collectors);
  }

  /**
   * Get a specific collector by name with type safety
   * @param name Collector name
   * @returns The collector instance or undefined if not found
   */
  getCollector<N extends CollectorName>(name: N): CollectorTypeMap[N] | undefined {
    return this.collectors.get(name) as CollectorTypeMap[N] | undefined;
  }

  /**
   * Get typed collectors for convenience
   * These methods provide type-safe access to specific collectors
   */
  getWorkflowCollector(): WorkflowMetricsCollector | undefined {
    return this.collectors.get('workflow') as WorkflowMetricsCollector | undefined;
  }

  getNodeCollector(): NodeMetricsCollector | undefined {
    return this.collectors.get('node') as NodeMetricsCollector | undefined;
  }

  getAgentCollector(): AgentMetricsCollector | undefined {
    return this.collectors.get('agent') as AgentMetricsCollector | undefined;
  }

  getEventCollector(): EventMetricsCollector | undefined {
    return this.collectors.get('event') as EventMetricsCollector | undefined;
  }

  getToolCollector(): ToolMetricsCollector | undefined {
    return this.collectors.get('tool') as ToolMetricsCollector | undefined;
  }

  getTokenCollector(): TokenMetricsCollector | undefined {
    return this.collectors.get('token') as TokenMetricsCollector | undefined;
  }

  getTemplateCollector(): TemplateMetricsCollector | undefined {
    return this.collectors.get('template') as TemplateMetricsCollector | undefined;
  }

  getConfigCollector(): ConfigMetricsCollector | undefined {
    return this.collectors.get('config') as ConfigMetricsCollector | undefined;
  }

  getErrorCollector(): ErrorMetricsCollector | undefined {
    return this.collectors.get('error') as ErrorMetricsCollector | undefined;
  }

  getResourceCollector(): ResourceMetricsCollector | undefined {
    return this.collectors.get('resource') as ResourceMetricsCollector | undefined;
  }

  getAgentLoopCollector(): AgentLoopMetricsCollector | undefined {
    return this.collectors.get('agentLoop') as AgentLoopMetricsCollector | undefined;
  }

  getTimeoutCollector(): TimeoutMetricsCollector | undefined {
    return this.collectors.get('timeout') as TimeoutMetricsCollector | undefined;
  }

  /**
   * Generate comprehensive report from all collectors
   */
  async generateReport(options?: {
    timeRange?: { from: number; to: number };
    includeTrends?: boolean;
  }): Promise<MetricReport> {
    const timestamp = Date.now();
    const timeRange = options?.timeRange;

    // Calculate total metrics and byType statistics with optional time range filter
    const { totalMetrics, byType } = this.calculateGlobalStats(timeRange);

    // Gather summary statistics from key collectors
    const workflowStats = this.getWorkflowCollector()?.getWorkflowUsageStats();
    const agentStats = this.getAgentCollector()?.getAgentStats();
    const eventSummary = this.getEventCollector()?.generateSummary();
    const nodeStats = this.getNodeCollector()?.getNodeExecutionStatsByType();

    // Build category statistics
    const byCategory: Record<string, number> = {};
    if (workflowStats) {
      byCategory['workflow'] = workflowStats.totalExecutions;
    }
    if (agentStats) {
      byCategory['agent'] = agentStats.totalExecutions;
    }
    if (eventSummary) {
      byCategory['event'] = eventSummary.totalEvents;
    }
    if (nodeStats) {
      byCategory['node'] = Object.values(nodeStats).reduce((sum, s) => sum + s.totalCount, 0);
    }

    // Get top metrics with optional time range filter
    const topMetrics = this.getTopMetricsAcrossCollectors(10, timeRange);

    // Detect anomalies
    const anomalies = this.detectAnomalies();

    // Calculate trends if requested
    let trends: Array<{
      metricName: string;
      dataPoints: Array<{ timestamp: number; value: number }>;
      trend: "increasing" | "decreasing" | "stable";
      changePercent?: number;
    }> | undefined;

    if (options?.includeTrends && timeRange) {
      trends = this.calculateTrends(timeRange);
    }

    const report: MetricReport = {
      timestamp,
      timeRange,
      summary: {
        totalMetrics,
        byType,
        byCategory,
      },
      topMetrics,
      anomalies,
      trends,
    };

    // Notify subscribers
    for (const subscriber of this.reportSubscribers) {
      try {
        await subscriber(report);
      } catch (error) {
        logger.error("Error in report subscriber", { error });
      }
    }

    return report;
  }

  /**
   * Subscribe to periodic reports
   */
  onReport(callback: (report: MetricReport) => void | Promise<void>): () => void {
    this.reportSubscribers.push(callback);
    return () => {
      const index = this.reportSubscribers.indexOf(callback);
      if (index > -1) {
        this.reportSubscribers.splice(index, 1);
      }
    };
  }

  /**
   * Flush all metrics from all collectors
   */
  async flushAll(): Promise<void> {
    logger.debug("Flushing all metrics collectors", { count: this.collectors.size });
    
    const flushPromises = Array.from(this.collectors.entries()).map(async ([name, collector]) => {
      try {
        await collector.flush();
      } catch (error) {
        logger.error(`Failed to flush collector: ${name}`, { error });
      }
    });
    
    await Promise.allSettled(flushPromises);
    logger.debug("All metrics flushed");
  }

  /**
   * Dispose all collectors and stop periodic reporting
   */
  dispose(): void {
    logger.info("Disposing Unified Metrics Manager");
    
    // Stop periodic reporting
    this.stopPeriodicReporting();

    // Dispose all collectors
    for (const [name, collector] of this.collectors.entries()) {
      try {
        collector.dispose();
        logger.debug(`Disposed collector: ${name}`);
      } catch (error) {
        logger.error(`Failed to dispose collector: ${name}`, { error });
      }
    }

    this.collectors.clear();
    this.reportSubscribers = [];
    logger.info("Unified Metrics Manager disposed");
  }

  /**
   * Setup periodic reporting
   */
  private setupPeriodicReporting(interval: number): void {
    if (this.isStarted) {
      logger.warn("Periodic reporting already started");
      return;
    }

    this.isStarted = true;
    this.reportingTimer = setInterval(async () => {
      try {
        await this.generateReport();
      } catch (error) {
        logger.error("Error generating periodic report", { error });
      }
    }, interval);

    logger.info("Periodic reporting enabled", { interval });
  }

  /**
   * Stop periodic reporting
   */
  private stopPeriodicReporting(): void {
    if (!this.isStarted) {
      return;
    }

    if (this.reportingTimer) {
      clearInterval(this.reportingTimer);
      this.reportingTimer = null;
    }

    this.isStarted = false;
    logger.info("Periodic reporting stopped");
  }

  /**
   * Calculate global statistics across all collectors
   */
  private calculateGlobalStats(timeRange?: { from: number; to: number }): { 
    totalMetrics: number;
    byType: Record<MetricType, number>;
  } {
    let totalMetrics = 0;
    const byType: Record<MetricType, number> = {
      counter: 0,
      gauge: 0,
      histogram: 0,
      summary: 0,
    };

    for (const [name, collector] of this.collectors.entries()) {
      try {
        // Query metrics with optional time range filter
        const filter: import('./types.js').MetricFilter = {};
        if (timeRange) {
          filter.timeRange = timeRange;
        }
        const result = collector.query(filter);
        totalMetrics += result.totalCount;

        // Count by type
        for (const metric of result.metrics.values()) {
          byType[metric.metricType] = (byType[metric.metricType] || 0) + 1;
        }
      } catch (error) {
        logger.warn(`Failed to calculate stats for collector: ${name}`, { error });
      }
    }

    return { totalMetrics, byType };
  }

  /**
   * Get top metrics across all collectors
   */
  private getTopMetricsAcrossCollectors(
    limit: number,
    timeRange?: { from: number; to: number }
  ): Array<{
    metricName: string;
    value: number;
    labels: Record<string, string>;
  }> {
    const allMetrics: Array<{
      metricName: string;
      value: number;
      labels: Record<string, string>;
      collector: string;
    }> = [];

    // Gather top metrics from each collector
    for (const [name, collector] of this.collectors.entries()) {
      try {
        const filter: import('./types.js').MetricFilter = { limit: 5 };
        if (timeRange) {
          filter.timeRange = timeRange;
        }
        const result = collector.query(filter);
        
        // Extract metrics with their actual labels from byLabel aggregation
        for (const [_metricKey, metric] of result.metrics.entries()) {
          if (typeof metric.value === 'number' && metric.value > 0) {
            const labels = this.extractLabelsFromAggregated(metric);
            
            allMetrics.push({
              metricName: metric.metricName,
              value: metric.value,
              labels,
              collector: name,
            });
          }
        }
      } catch (error) {
        logger.warn(`Failed to get top metrics from collector: ${name}`, { error });
      }
    }

    // Sort by value and return top N
    return allMetrics
      .sort((a, b) => b.value - a.value)
      .slice(0, limit)
      .map(({ metricName, value, labels }) => ({ metricName, value, labels }));
  }

  /**
   * Detect anomalies across all collectors
   * Basic implementation - can be extended with more sophisticated detection
   */
  private detectAnomalies(): Array<{
    metricName: string;
    description: string;
    severity: "low" | "medium" | "high";
  }> {
    const anomalies: Array<{
      metricName: string;
      description: string;
      severity: "low" | "medium" | "high";
    }> = [];

    // Check for high error rates
    const errorCollector = this.getErrorCollector();
    if (errorCollector) {
      const errorStats = errorCollector.query({});
      if (errorStats.totalCount > 100) {
        anomalies.push({
          metricName: 'error.occurrence.count',
          description: `High error count detected: ${errorStats.totalCount} errors`,
          severity: 'high',
        });
      }
    }

    // Check for workflow failure rate
    const workflowCollector = this.getWorkflowCollector();
    if (workflowCollector) {
      const stats = workflowCollector.getWorkflowUsageStats();
      if (stats.totalExecutions > 0 && stats.successRate < 0.8) {
        anomalies.push({
          metricName: 'workflow.execution.success.rate',
          description: `Low workflow success rate: ${(stats.successRate * 100).toFixed(2)}%`,
          severity: stats.successRate < 0.5 ? 'high' : 'medium',
        });
      }
    }

    return anomalies;
  }

  /**
   * Calculate trends for key metrics over a time range
   * 
   * This method aggregates data points for each metric across all collectors
   * and calculates the trend direction (increasing/decreasing/stable).
   * 
   * Design decisions:
   * - Iterates by metricName first, then collects from all collectors
   * - Only uses timeSeries data (not single aggregated values) for accurate trends
   * - Requires at least 2 data points to calculate a meaningful trend
   * - Uses simple percentage change between first and last data points
   */
  private calculateTrends(timeRange: { from: number; to: number }): Array<{
    metricName: string;
    dataPoints: Array<{ timestamp: number; value: number }>;
    trend: "increasing" | "decreasing" | "stable";
    changePercent?: number;
  }> {
    const trends: Array<{
      metricName: string;
      dataPoints: Array<{ timestamp: number; value: number }>;
      trend: "increasing" | "decreasing" | "stable";
      changePercent?: number;
    }> = [];

    // Key metrics to track for trends
    const keyMetrics = [
      'workflow.execution.count',
      'workflow.execution.duration',
      'node.execution.count',
      'agent.iteration.count',
      'event.processed.count',
      'tool.invocation.count',
      'token.usage.total',
      'error.occurrence.count',
    ];

    // Query each metric across all collectors and aggregate results
    for (const metricName of keyMetrics) {
      const allDataPoints: Array<{ timestamp: number; value: number }> = [];

      // Collect data points from all collectors for this specific metric
      for (const collector of this.collectors.values()) {
        try {
          const result = collector.query({
            metricName,
            timeRange,
          });

          if (result.metrics.size === 0) continue;

          // Aggregate data points from all matching metrics in this collector
          for (const [_metricKey, aggregated] of result.metrics.entries()) {
            if (aggregated.timeSeries && aggregated.timeSeries.length > 0) {
              allDataPoints.push(...aggregated.timeSeries);
            }
            // Note: We skip single aggregated values without time series
            // because they don't provide enough data points for trend analysis
          }
        } catch (error) {
          logger.warn(`Failed to query metric ${metricName} from collector`, { error });
        }
      }

      // Need at least 2 data points to calculate a trend
      if (allDataPoints.length < 2) continue;

      // Sort by timestamp
      allDataPoints.sort((a, b) => a.timestamp - b.timestamp);

      // Calculate trend
      const firstValue = allDataPoints[0]!.value;
      const lastValue = allDataPoints[allDataPoints.length - 1]!.value;
      const changePercent = firstValue !== 0 
        ? ((lastValue - firstValue) / Math.abs(firstValue)) * 100 
        : 0;

      let trend: "increasing" | "decreasing" | "stable";
      if (Math.abs(changePercent) < 5) {
        trend = "stable";
      } else if (changePercent > 0) {
        trend = "increasing";
      } else {
        trend = "decreasing";
      }

      trends.push({
        metricName,
        dataPoints: allDataPoints,
        trend,
        changePercent,
      });
    }

    return trends;
  }

  /**
   * Extract labels from an aggregated metric
   * Helper method to safely parse label keys from byLabel map
   */
  private extractLabelsFromAggregated(aggregated: import('./types.js').AggregatedMetric): Record<string, string> {
    if (!aggregated.byLabel || aggregated.byLabel.size === 0) {
      return {};
    }

    const firstLabelEntry = aggregated.byLabel.entries().next();
    if (firstLabelEntry.done) {
      return {};
    }

    try {
      return JSON.parse(firstLabelEntry.value[0]);
    } catch {
      logger.warn("Failed to parse label key", { labelKey: firstLabelEntry.value[0] });
      return {};
    }
  }
}
