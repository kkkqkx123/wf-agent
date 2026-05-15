/**
 * Metrics Registry
 * 
 * Manages all metrics collectors and provides centralized access.
 * Integrated into GlobalContext for easy access throughout the SDK.
 * 
 * Design Principles:
 * - Unified management of all metric collectors
 * - Centralized configuration and lifecycle management
 * - Aggregated reporting across all collectors
 * - Decoupled from EventRegistry (creates its own EventCollector)
 */

import type { GlobalContext } from "../global-context.js";
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
import type { MetricCollectorConfig, MetricReport, MetricType } from "./types.js";
import { BaseMetricCollector } from "./base-collector.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "MetricsRegistry" });

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
  enablePeriodicReporting?: boolean;
  reportingInterval?: number;
}

export class MetricsRegistry {
  private collectors: Map<string, BaseMetricCollector>;
  private reportSubscribers: Array<(report: MetricReport) => void | Promise<void>> = [];
  private reportingTimer: NodeJS.Timeout | null = null;
  private isStarted: boolean = false;

  constructor(
    private globalContext: GlobalContext,
    config?: MetricsRegistryConfig
  ) {
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
   * Get a specific collector by name
   */
  getCollector<T extends BaseMetricCollector>(name: string): T | undefined {
    return this.collectors.get(name) as T | undefined;
  }

  /**
   * Get typed collectors for convenience
   */
  getWorkflowCollector(): WorkflowMetricsCollector {
    return this.collectors.get('workflow') as WorkflowMetricsCollector;
  }

  getNodeCollector(): NodeMetricsCollector {
    return this.collectors.get('node') as NodeMetricsCollector;
  }

  getAgentCollector(): AgentMetricsCollector {
    return this.collectors.get('agent') as AgentMetricsCollector;
  }

  getEventCollector(): EventMetricsCollector {
    return this.collectors.get('event') as EventMetricsCollector;
  }

  getToolCollector(): ToolMetricsCollector {
    return this.collectors.get('tool') as ToolMetricsCollector;
  }

  getTokenCollector(): TokenMetricsCollector {
    return this.collectors.get('token') as TokenMetricsCollector;
  }

  getTemplateCollector(): TemplateMetricsCollector {
    return this.collectors.get('template') as TemplateMetricsCollector;
  }

  getConfigCollector(): ConfigMetricsCollector {
    return this.collectors.get('config') as ConfigMetricsCollector;
  }

  getErrorCollector(): ErrorMetricsCollector {
    return this.collectors.get('error') as ErrorMetricsCollector;
  }

  getResourceCollector(): ResourceMetricsCollector {
    return this.collectors.get('resource') as ResourceMetricsCollector;
  }

  getAgentLoopCollector(): AgentLoopMetricsCollector {
    return this.collectors.get('agentLoop') as AgentLoopMetricsCollector;
  }

  /**
   * Generate comprehensive report from all collectors
   */
  async generateReport(options?: {
    timeRange?: { from: number; to: number };
    includeTrends?: boolean;
  }): Promise<MetricReport> {
    const timestamp = Date.now();

    // Calculate total metrics and byType statistics
    const { totalMetrics, byType } = this.calculateGlobalStats();

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

    const report: MetricReport = {
      timestamp,
      summary: {
        totalMetrics,
        byType,
        byCategory,
      },
      topMetrics: this.getTopMetricsAcrossCollectors(10),
      anomalies: this.detectAnomalies(),
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
  private calculateGlobalStats(): { 
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
        // Query all metrics from this collector
        const result = collector.query({});
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
  private getTopMetricsAcrossCollectors(limit: number): Array<{
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
        const result = collector.query({ limit: 5 });
        for (const metric of result.metrics.values()) {
          if (typeof metric.value === 'number' && metric.value > 0) {
            allMetrics.push({
              metricName: metric.metricName,
              value: metric.value,
              labels: {},
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
}
