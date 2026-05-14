/**
 * Metrics Registry
 * 
 * Manages all metrics collectors and provides centralized access.
 * Integrated into GlobalContext for easy access throughout the SDK.
 */

import type { GlobalContext } from "../global-context.js";
import { WorkflowMetricsCollector } from "./workflow-metrics-collector.js";
import { NodeMetricsCollector } from "./node-metrics-collector.js";
import { AgentMetricsCollector } from "./agent-metrics-collector.js";
import type { EventMetricsCollector } from "./event-collector.js";
import type { MetricCollectorConfig, MetricReport } from "./types.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "MetricsRegistry" });

export interface MetricsRegistryConfig {
  workflowMetrics?: MetricCollectorConfig;
  nodeMetrics?: MetricCollectorConfig;
  agentMetrics?: MetricCollectorConfig;
  enablePeriodicReporting?: boolean;
  reportingInterval?: number;
}

export class MetricsRegistry {
  private workflowMetrics: WorkflowMetricsCollector;
  private nodeMetrics: NodeMetricsCollector;
  private agentMetrics: AgentMetricsCollector;
  private eventCollector: EventMetricsCollector | null = null;
  private reportSubscribers: Array<(report: MetricReport) => void | Promise<void>> = [];
  private reportingTimer: NodeJS.Timeout | null = null;

  constructor(
    private globalContext: GlobalContext,
    config?: MetricsRegistryConfig
  ) {
    this.workflowMetrics = new WorkflowMetricsCollector(config?.workflowMetrics);
    this.nodeMetrics = new NodeMetricsCollector(config?.nodeMetrics);
    this.agentMetrics = new AgentMetricsCollector(config?.agentMetrics);

    // Get event collector from event registry
    try {
      this.eventCollector = this.globalContext.eventRegistry.getMetricsCollector();
    } catch (error) {
      logger.warn("Event metrics collector not available", { error });
    }

    // Setup periodic reporting if enabled
    if (config?.enablePeriodicReporting) {
      this.setupPeriodicReporting(config.reportingInterval || 60000); // Default: 1 minute
    }

    logger.info("Unified Metrics Manager initialized");
  }

  /**
   * Get all collectors
   */
  getCollectors(): {
    workflow: WorkflowMetricsCollector;
    node: NodeMetricsCollector;
    agent: AgentMetricsCollector;
    event: EventMetricsCollector | null;
  } {
    return {
      workflow: this.workflowMetrics,
      node: this.nodeMetrics,
      agent: this.agentMetrics,
      event: this.eventCollector,
    };
  }

  /**
   * Generate comprehensive report from all collectors
   */
  async generateReport(options?: {
    timeRange?: { from: number; to: number };
    includeTrends?: boolean;
  }): Promise<MetricReport> {
    const timestamp = Date.now();

    // Gather summary statistics
    const workflowStats = this.workflowMetrics.getWorkflowUsageStats();
    const agentStats = this.agentMetrics.getAgentStats();
    const eventSummary = this.eventCollector?.generateSummary();

    const report: MetricReport = {
      timestamp,
      summary: {
        totalMetrics: this.getTotalMetricCount(),
        byType: {
          counter: 0,
          gauge: 0,
          histogram: 0,
          summary: 0,
        },
        byCategory: {
          workflow: workflowStats.totalExecutions,
          agent: agentStats.totalExecutions,
          event: eventSummary?.totalEvents || 0,
        },
      },
      topMetrics: [
        ...this.workflowMetrics.getTopWorkflows(5).map(wf => ({
          metricName: 'workflow.execution.count',
          value: wf.executionCount,
          labels: { workflow_id: wf.workflowId },
        })),
        ...this.nodeMetrics.getTopNodeTemplates(5).map(nt => ({
          metricName: 'node.template.instantiation.count',
          value: nt.instantiationCount,
          labels: { template_name: nt.templateName },
        })),
      ],
      anomalies: [],
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
   * Flush all metrics
   */
  async flushAll(): Promise<void> {
    logger.debug("Flushing all metrics collectors");
    
    await Promise.all([
      this.workflowMetrics.flush(),
      this.nodeMetrics.flush(),
      this.agentMetrics.flush(),
      this.eventCollector?.flush(),
    ].filter(Boolean));
  }

  /**
   * Dispose all collectors and stop periodic reporting
   */
  dispose(): void {
    logger.info("Disposing Unified Metrics Manager");
    
    if (this.reportingTimer) {
      clearInterval(this.reportingTimer);
      this.reportingTimer = null;
    }

    this.workflowMetrics.dispose();
    this.nodeMetrics.dispose();
    this.agentMetrics.dispose();
    this.reportSubscribers = [];
  }

  /**
   * Setup periodic reporting
   */
  private setupPeriodicReporting(interval: number): void {
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
   * Get total metric count across all collectors
   */
  private getTotalMetricCount(): number {
    // This would query each collector and sum up
    // For now, return a placeholder
    return 0;
  }
}
