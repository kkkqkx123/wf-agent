/**
 * Agent Loop Metrics Collector
 * 
 * Tracks agent loop execution metrics including:
 * - Execution count by profile and config
 * - Iteration count distributions
 * - Tool call frequency
 * - Token usage
 */

import { BaseMetricCollector } from "./base-collector.js";
import type { MetricCollectorConfig } from "./types.js";
import { PrometheusFormatter, type PrometheusMetric } from "./utils/prometheus-formatter.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "AgentMetricsCollector" });

export class AgentMetricsCollector extends BaseMetricCollector {
  constructor(config?: MetricCollectorConfig) {
    super(config);
  }

  /**
   * Record agent loop execution start
   */
  recordExecutionStart(profileId: string, agentConfigId: string, executionId: string): void {
    if (!profileId || !executionId) {
      logger.warn("recordExecutionStart called with missing parameters");
      return;
    }

    this.incrementCounter('agent.loop.execution.count', {
      profile_id: profileId,
      agent_config_id: agentConfigId,
      execution_id: executionId,
    });
  }

  /**
   * Record agent loop completion
   */
  recordExecutionComplete(profileId: string, result: {
    iterations: number;
    toolCallCount: number;
    duration: number;
    tokenUsage?: number;
    success: boolean;
  }): void {
    if (!profileId) {
      logger.warn("recordExecutionComplete called with missing profileId");
      return;
    }

    // Record duration
    this.observeHistogram('agent.loop.duration', result.duration, {
      profile_id: profileId,
    });

    // Record iteration count
    this.observeHistogram('agent.loop.iterations_per_execution', result.iterations, {
      profile_id: profileId,
    });

    // Record token usage per iteration
    if (result.tokenUsage !== undefined && result.iterations > 0) {
      this.observeHistogram('agent.loop.tokens_per_iteration', 
        result.tokenUsage / result.iterations, {
        profile_id: profileId,
      });
    }

    logger.debug("Recorded agent loop completion", { 
      profileId, 
      iterations: result.iterations,
      duration: result.duration 
    });
  }

  /**
   * Record iteration
   */
  recordIteration(profileId: string, iteration: number): void {
    if (!profileId) {
      logger.warn("recordIteration called with missing profileId");
      return;
    }

    this.incrementCounter('agent.loop.iteration.count', {
      profile_id: profileId,
      iteration_number: iteration.toString(),
    });
  }

  /**
   * Record tool call
   */
  recordToolCall(toolName: string, profileId: string, result: {
    success: boolean;
    duration: number;
  }): void {
    if (!toolName || !profileId) {
      logger.warn("recordToolCall called with missing parameters");
      return;
    }

    this.incrementCounter('agent.tool.call.count', {
      tool_name: toolName,
      profile_id: profileId,
    });

    this.observeHistogram('agent.tool.execution.duration', result.duration, {
      tool_name: toolName,
    });
  }

  /**
   * Get agent loop statistics
   */
  getAgentStats(profileId?: string): {
    totalExecutions: number;
    avgIterations: number;
    byProfile: Record<string, number>;
  } {
    const filter = profileId ? {
      labels: { profile_id: profileId }
    } : {};

    // Query execution count
    const countResult = this.query({
      metricName: 'agent.loop.execution.count',
      metricType: 'counter',
      ...filter,
    });

    // Query iteration histogram
    const iterationResult = this.query({
      metricName: 'agent.loop.iterations_per_execution',
      metricType: 'histogram',
      ...filter,
    });

    const countMetric = countResult.metrics.get('agent.loop.execution.count');
    const totalExecutions = countMetric ? countMetric.value : 0;

    // Calculate average iterations
    let avgIterations = 0;
    const iterationMetric = iterationResult.metrics.get('agent.loop.iterations_per_execution');
    if (iterationMetric && iterationMetric.timeSeries && iterationMetric.timeSeries.length > 0) {
      const values = iterationMetric.timeSeries.map(ts => ts.value);
      avgIterations = values.reduce((sum, v) => sum + v, 0) / values.length;
    }

    // Group by profile
    const byProfile: Record<string, number> = {};
    if (countMetric) {
      for (const [labelKey, labelAgg] of countMetric.byLabel.entries()) {
        try {
          const labels = JSON.parse(labelKey);
          if (labels.profile_id) {
            byProfile[labels.profile_id] = (byProfile[labels.profile_id] || 0) + labelAgg.value;
          }
        } catch (error) {
          logger.warn("Failed to parse label key", { labelKey, error });
        }
      }
    }

    return {
      totalExecutions,
      avgIterations,
      byProfile,
    };
  }

  /**
   * Export agent metrics in Prometheus format
   */
  toPrometheus(): string[] {
    const stats = this.getAgentStats();
    const metrics: PrometheusMetric[] = [];
    
    // Total executions
    metrics.push({
      name: 'agent_loop_execution_total',
      type: 'counter',
      help: 'Total agent loop executions',
      samples: [{ value: stats.totalExecutions }]
    });
    
    // Average iterations
    metrics.push({
      name: 'agent_loop_iterations_avg',
      type: 'gauge',
      help: 'Average iterations per agent loop',
      samples: [{ value: stats.avgIterations }]
    });
    
    // By profile
    for (const [profileId, count] of Object.entries(stats.byProfile)) {
      metrics.push({
        name: 'agent_loop_execution_by_profile_total',
        type: 'counter',
        help: 'Agent loop executions by profile',
        samples: [{ labels: { profile_id: profileId }, value: count }]
      });
    }
    
    return metrics.flatMap(m => PrometheusFormatter.formatMetric(m));
  }
  
  /**
   * Export as JSON
   */
  toJSON(): Record<string, unknown> {
    return {
      type: 'agent',
      stats: this.getAgentStats()
    };
  }
}
