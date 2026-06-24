/**
 * Performance Metrics API - Track and analyze execution performance
 *
 * Provides comprehensive performance analysis including:
 * - Timeline analysis with token usage, cost, and memory metrics
 * - Execution comparison and improvement analysis
 * - Tool execution breakdown
 * - Performance recommendations
 */

import type { APIDependencyManager } from "../../core/sdk-dependencies.js";
import type { ID } from "@wf-agent/types";
import type { IterationSystemMetrics, IterationLLMMetrics } from "../../../agent/resources/agent-loop-iteration-api.js";
import type { PersistenceLayer } from "../../core/persistence-interfaces.js";
import { NoOpPersistenceLayer } from "../../core/__tests__/no-op-persistence.js";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "PerformanceMetricsAPI" });

/**
 * Complete performance timeline for an execution
 * Aggregates both system and LLM metrics for comprehensive analysis
 */
export interface PerformanceTimeline {
  /** Execution ID */
  executionId: ID;

  /** Total tokens used (input + output) */
  totalTokens: number;

  /** Total cost in USD */
  totalCost: number;

  /** Average time per iteration (ms) */
  averageIterationTime: number;

  /** Peak memory usage (megabytes) */
  peakMemoryUsage: number;

  /** Tool execution time breakdown by tool name */
  toolExecutionBreakdown: Record<string, number>;

  /** System metrics timeline */
  systemMetrics: IterationSystemMetrics[];

  /** LLM metrics timeline */
  llmMetrics: IterationLLMMetrics[];

  /** Time range of execution */
  timeRange: { start: number; end: number };
}

/**
 * Comparison result between two executions
 */
export interface ExecutionComparison {
  /** Baseline execution metrics */
  baseline: PerformanceTimeline;

  /** Target execution metrics */
  target: PerformanceTimeline;

  /** Performance improvements (percentage) */
  improvements: {
    /** Token usage reduction (%) */
    tokenReduction: number;

    /** Cost reduction (%) */
    costReduction: number;

    /** Execution speed improvement (%) */
    speedImprovement: number;

    /** Memory usage improvement (%) */
    memoryImprovement: number;
  };

  /** AI-generated recommendations for optimization */
  recommendations: string[];
}

/**
 * Performance Metrics API Implementation
 *
 * Provides performance tracking and analysis for agent loop executions.
 * Uses separate system and LLM metrics for detailed performance insights.
 */
export class PerformanceMetricsAPI {
  private persistence: PersistenceLayer;

  constructor(deps: APIDependencyManager) {
    this.persistence = deps.getPersistenceLayer?.() || new NoOpPersistenceLayer();
  }

  /**
   * Get the complete performance timeline for an execution
   *
   * Analyzes all system and LLM metrics for an execution and generates
   * a comprehensive performance report including costs, times, and memory.
   *
   * @param executionId The execution ID
   * @returns Performance timeline with detailed metrics
   */
  async getPerformanceTimeline(executionId: ID): Promise<PerformanceTimeline> {
    try {
      const [systemMetrics, llmMetrics] = await Promise.all([
        this.persistence.getSystemMetrics(executionId),
        this.persistence.getLLMMetrics(executionId),
      ]);

      const systemMetricsArray = systemMetrics || [];
      const llmMetricsArray = llmMetrics || [];

      logger.debug("Retrieved performance metrics", {
        executionId,
        systemMetrics: systemMetricsArray.length,
        llmMetrics: llmMetricsArray.length,
      });

      // Empty execution case
      if (systemMetricsArray.length === 0 && llmMetricsArray.length === 0) {
        return {
          executionId,
          totalTokens: 0,
          totalCost: 0,
          averageIterationTime: 0,
          peakMemoryUsage: 0,
          toolExecutionBreakdown: {},
          systemMetrics: [],
          llmMetrics: [],
          timeRange: { start: 0, end: 0 },
        };
      }

      return {
        executionId,
        totalTokens: this.sumTokens(llmMetricsArray),
        totalCost: this.sumCost(llmMetricsArray),
        averageIterationTime: this.calculateAvgIterationTime(systemMetricsArray),
        peakMemoryUsage: this.calculatePeakMemory(systemMetricsArray),
        toolExecutionBreakdown: this.breakdownToolExecution(llmMetricsArray),
        systemMetrics: systemMetricsArray,
        llmMetrics: llmMetricsArray,
        timeRange: {
          start: systemMetricsArray.length > 0 ? systemMetricsArray[0]!.timestamp : 0,
          end: systemMetricsArray.length > 0 ? systemMetricsArray[systemMetricsArray.length - 1]!.timestamp : 0,
        },
      };
    } catch (error) {
      logger.error("Failed to get performance timeline", { executionId, error });
      throw error;
    }
  }

  /**
   * Compare performance metrics between two executions
   *
   * Analyzes the differences between baseline and target executions,
   * calculating improvement percentages and generating optimization
   * recommendations.
   *
   * @param baselineExecutionId The baseline execution ID
   * @param targetExecutionId The target execution ID for comparison
   * @returns Comparison result with improvement analysis
   */
  async compareExecutions(
    baselineExecutionId: ID,
    targetExecutionId: ID,
  ): Promise<ExecutionComparison> {
    try {
      const [baseline, target] = await Promise.all([
        this.getPerformanceTimeline(baselineExecutionId),
        this.getPerformanceTimeline(targetExecutionId),
      ]);

      logger.debug("Comparing executions", {
        baseline: baselineExecutionId,
        target: targetExecutionId,
      });

      const tokenReduction = this.calculateReduction(baseline.totalTokens, target.totalTokens);
      const costReduction = this.calculateReduction(baseline.totalCost, target.totalCost);
      const speedImprovement = this.calculateReduction(
        baseline.averageIterationTime,
        target.averageIterationTime,
      );
      const memoryImprovement = this.calculateReduction(
        baseline.peakMemoryUsage,
        target.peakMemoryUsage,
      );

      const improvements = {
        tokenReduction,
        costReduction,
        speedImprovement,
        memoryImprovement,
      };

      return {
        baseline,
        target,
        improvements,
        recommendations: this.generateRecommendations(baseline, target, improvements),
      };
    } catch (error) {
      logger.error("Failed to compare executions", {
        baseline: baselineExecutionId,
        target: targetExecutionId,
        error,
      });
      throw error;
    }
  }

  /**
   * Get performance summary statistics for an execution
   *
   * Provides quick access to key performance indicators without
   * retrieving the full timeline.
   *
   * @param executionId The execution ID
   * @returns Summary statistics
   */
  async getPerformanceSummary(executionId: ID): Promise<{
    totalTokens: number;
    totalCost: number;
    peakMemory: number;
    duration: number;
    iterationCount: number;
  }> {
    const timeline = await this.getPerformanceTimeline(executionId);
    const duration = timeline.timeRange.end - timeline.timeRange.start;

    return {
      totalTokens: timeline.totalTokens,
      totalCost: timeline.totalCost,
      peakMemory: timeline.peakMemoryUsage,
      duration,
      iterationCount: timeline.systemMetrics.length,
    };
  }

  /**
   * Identify performance bottlenecks for an execution
   *
   * Analyzes the performance timeline to identify which tools or phases
   * consume the most time and resources.
   *
   * @param executionId The execution ID
   * @returns List of bottlenecks ranked by impact
   */
  async identifyBottlenecks(
    executionId: ID,
  ): Promise<Array<{ name: string; percentage: number; time: number }>> {
    const timeline = await this.getPerformanceTimeline(executionId);

    if (timeline.systemMetrics.length === 0) {
      return [];
    }

    // Calculate total execution time
    const totalDuration = timeline.systemMetrics.reduce((sum, metric) => {
      return sum + metric.durationMs;
    }, 0);

    if (totalDuration === 0) {
      return [];
    }

    // Rank by duration (time spent in each metric)
    return timeline.systemMetrics
      .map(metric => ({
        name: `iteration_${metric.iteration}`,
        time: metric.durationMs,
        percentage: (metric.durationMs / totalDuration) * 100,
      }))
      .sort((a, b) => b.percentage - a.percentage)
      .slice(0, 10); // Top 10 bottlenecks
  }

  // ============================================================================
  // Helper Methods
  // ============================================================================

  private sumTokens(llmMetrics: IterationLLMMetrics[]): number {
    return llmMetrics.reduce((sum, m) => {
      return sum + m.inputTokens + m.outputTokens;
    }, 0);
  }

  private sumCost(llmMetrics: IterationLLMMetrics[]): number {
    return llmMetrics.reduce((sum, m) => sum + m.costUsd, 0);
  }

  private calculateAvgIterationTime(systemMetrics: IterationSystemMetrics[]): number {
    if (systemMetrics.length === 0) return 0;

    const totalTime = systemMetrics.reduce((sum, m) => {
      return sum + m.durationMs;
    }, 0);

    return totalTime / systemMetrics.length;
  }

  private calculatePeakMemory(systemMetrics: IterationSystemMetrics[]): number {
    if (systemMetrics.length === 0) return 0;
    return Math.max(...systemMetrics.map((m) => m.memoryPeakMb));
  }

  private breakdownToolExecution(llmMetrics: IterationLLMMetrics[]): Record<string, number> {
    const breakdown: Record<string, number> = {};

    // Aggregate metrics by model
    llmMetrics.forEach((metric) => {
      const modelKey = metric.model || "unknown";
      breakdown[modelKey] = (breakdown[modelKey] ?? 0) + metric.durationMs;
    });

    return breakdown;
  }

  private calculateReduction(baseline: number, target: number): number {
    if (baseline === 0) {
      return target === 0 ? 0 : -100; // If baseline is 0, any increase is -100%
    }
    return ((baseline - target) / baseline) * 100;
  }

  private generateRecommendations(
    baseline: PerformanceTimeline,
    target: PerformanceTimeline,
    improvements: {
      tokenReduction: number;
      costReduction: number;
      speedImprovement: number;
      memoryImprovement: number;
    },
  ): string[] {
    const recommendations: string[] = [];

    // Token usage recommendations
    if (improvements.tokenReduction > 20) {
      recommendations.push("✅ Excellent token usage reduction (>20%)");
    } else if (improvements.tokenReduction > 10) {
      recommendations.push("✅ Good token usage reduction (>10%)");
    } else if (improvements.tokenReduction < -20) {
      recommendations.push("⚠️  Token usage increased significantly (>20%), consider optimization");
    } else if (improvements.tokenReduction < 0) {
      recommendations.push(
        "⚠️  Token usage increased, review prompt engineering and context handling",
      );
    }

    // Cost recommendations
    if (improvements.costReduction > 15) {
      recommendations.push("✅ Significant cost reduction");
    } else if (improvements.costReduction < -15) {
      recommendations.push("⚠️  Cost increased, evaluate model selection");
    }

    // Speed recommendations
    if (improvements.speedImprovement > 25) {
      recommendations.push("✅ Major performance improvement in execution speed");
    } else if (improvements.speedImprovement > 10) {
      recommendations.push("✅ Good performance improvement");
    } else if (improvements.speedImprovement < -25) {
      recommendations.push("⚠️  Significant slowdown detected, investigate root causes");
    }

    // Memory recommendations
    if (improvements.memoryImprovement > 20) {
      recommendations.push("✅ Significant memory usage reduction");
    } else if (improvements.memoryImprovement < -20) {
      recommendations.push("⚠️  Memory usage increased substantially, check for memory leaks");
    }

    // Comparative analysis
    if (
      improvements.tokenReduction > 0 &&
      improvements.costReduction > 0 &&
      improvements.speedImprovement > 0
    ) {
      recommendations.push("🎯 All metrics improved - excellent optimization!");
    } else if (Object.values(improvements).some((v) => v > 0)) {
      recommendations.push("📊 Some metrics improved, focus on remaining areas");
    } else {
      recommendations.push("📋 No significant improvements detected, deeper analysis recommended");
    }

    // Detailed bottleneck analysis
    const targetTotalTime =
      target.averageIterationTime * target.systemMetrics.length ||
      target.systemMetrics.reduce(
        (sum, m) => sum + m.durationMs,
        0,
      );
    const baselineTotalTime =
      baseline.averageIterationTime * baseline.systemMetrics.length ||
      baseline.systemMetrics.reduce(
        (sum, m) => sum + m.durationMs,
        0,
      );

    if (targetTotalTime > 0 && baselineTotalTime > 0) {
      const timeRatio = targetTotalTime / baselineTotalTime;
      if (timeRatio > 1.3) {
        recommendations.push("🔍 Consider profiling to identify performance bottlenecks");
      }
    }

    return recommendations.length > 0
      ? recommendations
      : ["ℹ️  Baseline and target metrics are comparable"];
  }
}
