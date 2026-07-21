/**
 * Performance Utility Functions
 *
 * Shared utility functions for performance analysis across Agent and Workflow domains.
 * Extracted from duplicated implementations in AgentPerformanceAnalysisAPI,
 * WorkflowPerformanceAnalysisAPI, and PerformanceMetricsAPI.
 */

import type {
  PerformanceTier,
  PerformanceSummary,
  PerformanceBottleneck,
  PerformanceTrend,
} from "../resources/metrics/performance-metrics-api.js";

/**
 * Base execution item interface for performance calculation
 */
export interface PerformanceExecItem {
  duration: number;
  success: boolean;
  toolCallCount: number;
  operations: Array<{ type: string; duration: number; name: string }>;
}

/**
 * Classify performance into tiers
 *
 * @param duration - Duration in milliseconds
 * @returns Performance tier classification
 */
export function classifyPerformance(duration: number): PerformanceTier {
  const fastThreshold = 1000; // 1 second
  const normalThreshold = 5000; // 5 seconds

  if (duration < fastThreshold) {
    return "fast";
  } else if (duration < normalThreshold) {
    return "normal";
  } else {
    return "slow";
  }
}

/**
 * Calculate performance summary
 *
 * @param executions - Array of execution items (iterations or nodes)
 * @param totalDuration - Total execution duration in milliseconds
 * @param entityLabel - Label for the entity type (e.g., "iteration", "node")
 * @returns Performance summary with recommendations
 */
export function calculatePerformanceSummary(
  executions: PerformanceExecItem[],
  totalDuration: number,
  entityLabel: string = "iteration",
): PerformanceSummary {
  if (executions.length === 0) {
    return {
      avgIterationDuration: 0,
      minIterationDuration: 0,
      maxIterationDuration: 0,
      successRate: 0,
      operationsPerSecond: 0,
      recommendations: [],
    };
  }

  const durations = executions.map((i) => i.duration);
  const avgDuration = Math.round(durations.reduce((a, b) => a + b, 0) / durations.length);
  const minDuration = Math.min(...durations);
  const maxDuration = Math.max(...durations);

  const successCount = executions.filter((i) => i.success).length;
  const successRate = (successCount / executions.length) * 100;

  const totalToolCalls = executions.reduce((sum, i) => sum + i.toolCallCount, 0);
  const operationsPerSecond =
    totalDuration > 0 ? Math.round((totalToolCalls / totalDuration) * 1000) : 0;

  // Generate recommendations
  const recommendations: string[] = [];

  if (maxDuration / avgDuration > 2) {
    recommendations.push(
      `High variance in ${entityLabel} duration detected. Investigate slow ${entityLabel}s.`,
    );
  }

  if (operationsPerSecond < 1 && totalToolCalls > 0) {
    recommendations.push("Low throughput. Consider optimizing tool execution.");
  }

  if (successRate < 90) {
    recommendations.push("High failure rate. Review error logs for patterns.");
  }

  return {
    avgIterationDuration: avgDuration,
    minIterationDuration: minDuration,
    maxIterationDuration: maxDuration,
    avgToolCallDuration: totalToolCalls > 0 ? Math.round(totalDuration / totalToolCalls) : 0,
    successRate: Math.round(successRate * 100) / 100,
    operationsPerSecond,
    recommendations,
  };
}

/**
 * Identify performance bottlenecks
 *
 * @param executions - Array of execution items (iterations or nodes)
 * @param totalDuration - Total execution duration in milliseconds
 * @param getLocationLabel - Function to extract location label from an execution item
 * @param locationType - Type label for the location (e.g., "iteration", "node")
 * @returns Array of bottlenecks sorted by impact, limited to top 10
 */
export function identifyBottlenecks<T extends PerformanceExecItem>(
  executions: T[],
  totalDuration: number,
  getLocationLabel: (execution: T, index: number) => string | number,
  locationType: string = "iteration",
): PerformanceBottleneck[] {
  const bottlenecks: PerformanceBottleneck[] = [];

  if (executions.length === 0) {
    return bottlenecks;
  }

  // Find slow executions
  const execDurations = executions.map((e) => e.duration);
  const avgExecDuration = execDurations.reduce((a, b) => a + b, 0) / execDurations.length;

  for (const exec of executions) {
    if (exec.duration > avgExecDuration * 1.5) {
      bottlenecks.push({
        type: locationType as PerformanceBottleneck["type"],
        location: String(getLocationLabel(exec, executions.indexOf(exec))),
        duration: exec.duration,
        percentage: totalDuration > 0 ? (exec.duration / totalDuration) * 100 : 0,
        severity: exec.duration > avgExecDuration * 2.5 ? "high" : "medium",
      });
    }

    // Find slow tool calls
    for (const operation of exec.operations) {
      if (operation.type === "tool_call" && operation.duration > 1000) {
        bottlenecks.push({
          type: "tool_call",
          location: operation.name,
          duration: operation.duration,
          percentage: totalDuration > 0 ? (operation.duration / totalDuration) * 100 : 0,
          severity: operation.duration > 5000 ? "high" : "medium",
        });
      }
    }
  }

  return bottlenecks.sort((a, b) => b.duration - a.duration).slice(0, 10);
}

/**
 * Calculate performance trend by comparing first half to second half
 *
 * @param durations - Array of durations in milliseconds
 * @returns Performance trend classification
 */
export function calculateTrend(durations: number[]): PerformanceTrend {
  if (durations.length < 2) {
    return "stable";
  }

  const first = durations.slice(0, Math.floor(durations.length / 2));
  const last = durations.slice(Math.floor(durations.length / 2));
  const firstAvg = first.length > 0 ? first.reduce((a, b) => a + b, 0) / first.length : 0;
  const lastAvg = last.length > 0 ? last.reduce((a, b) => a + b, 0) / last.length : 0;

  if (lastAvg < firstAvg * 0.8) {
    return "improving";
  } else if (lastAvg > firstAvg * 1.2) {
    return "degrading";
  }
  return "stable";
}

/**
 * Calculate variance of durations
 *
 * @param durations - Array of durations in milliseconds
 * @returns Variance value
 */
export function calculateVariance(durations: number[]): number {
  if (durations.length === 0) return 0;
  const average = durations.reduce((a, b) => a + b, 0) / durations.length;
  return durations.reduce((sum, d) => sum + Math.pow(d - average, 2), 0) / durations.length;
}