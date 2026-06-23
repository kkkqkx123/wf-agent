/**
 * Performance Metrics API - Track and analyze execution performance
 *
 * Provides comprehensive performance analysis including:
 * - Timeline analysis with token usage, cost, and memory metrics
 * - Execution comparison and improvement analysis
 * - Tool execution breakdown
 * - Performance recommendations
 */

import { QueryableResourceAPI } from "../generic-resource-api.js";
import type { APIDependencyManager } from "../../core/sdk-dependencies.js";
import type { ID } from "@wf-agent/types";
import type { ResourceUsageRecord } from "../../../agent/resources/agent-loop-iteration-api.js";
import type { PersistenceLayer } from "../../core/persistence-interfaces.js";
import { NoOpPersistenceLayer } from "../../core/persistence-interfaces.js";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "PerformanceMetricsAPI" });

/**
 * Complete performance timeline for an execution
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

  /** Peak memory usage in bytes */
  peakMemoryUsage: number;

  /** Tool execution time breakdown by tool name */
  toolExecutionBreakdown: Record<string, number>;

  /** Complete timeline of resource usage records */
  timeline: ResourceUsageRecord[];

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
 * Leverages the persistence layer for durable metrics storage.
 */
export class PerformanceMetricsAPI extends QueryableResourceAPI<
  ResourceUsageRecord,
  string,
  { executionId?: ID }
> {
  private persistence: PersistenceLayer;

  constructor(deps: APIDependencyManager) {
    super();
    this.persistence = deps.getPersistenceLayer?.() || new NoOpPersistenceLayer();
  }

  protected async getResource(): Promise<ResourceUsageRecord | null> {
    // Not applicable for performance metrics
    return null;
  }

  protected async getAllResources(): Promise<ResourceUsageRecord[]> {
    // Not applicable for performance metrics
    return [];
  }

  /**
   * Get the complete performance timeline for an execution
   *
   * Analyzes all resource usage records for an execution and generates
   * a comprehensive performance report including costs, times, and memory.
   *
   * @param executionId The execution ID
   * @returns Performance timeline with detailed metrics
   */
  async getPerformanceTimeline(executionId: ID): Promise<PerformanceTimeline> {
    try {
      const records = await this.persistence.getResourceUsageRecords(executionId);

      logger.debug("Retrieved resource usage records", {
        executionId,
        count: records.length,
      });

      // Empty execution case
      if (records.length === 0) {
        return {
          executionId,
          totalTokens: 0,
          totalCost: 0,
          averageIterationTime: 0,
          peakMemoryUsage: 0,
          toolExecutionBreakdown: {},
          timeline: [],
          timeRange: { start: 0, end: 0 },
        };
      }

      return {
        executionId,
        totalTokens: this.sumTokens(records),
        totalCost: this.sumCost(records),
        averageIterationTime: this.calculateAvgIterationTime(records),
        peakMemoryUsage: this.calculatePeakMemory(records),
        toolExecutionBreakdown: this.breakdownToolExecution(records),
        timeline: records,
        timeRange: {
          start: 0,
          end: records.length > 0 ? records.length * 1000 : 0, // Approximate based on record count
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
      iterationCount: timeline.timeline.length,
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

    if (timeline.timeline.length === 0) {
      return [];
    }

    // Calculate total time spent in tools
    const totalToolTime = timeline.timeline.reduce((sum, record) => {
      return sum + (record.timingBreakdown?.toolExecutionTime ?? 0);
    }, 0);

    if (totalToolTime === 0) {
      return [];
    }

    // Rank tools by time spent
    return Object.entries(timeline.toolExecutionBreakdown)
      .map(([name, time]) => ({
        name,
        time: time as number,
        percentage: ((time as number) / totalToolTime) * 100,
      }))
      .sort((a, b) => b.percentage - a.percentage)
      .slice(0, 10); // Top 10 bottlenecks
  }

  // ============================================================================
  // Helper Methods
  // ============================================================================

  private sumTokens(records: ResourceUsageRecord[]): number {
    return records.reduce((sum, r) => {
      const input = r.llmInputTokens ?? 0;
      const output = r.llmOutputTokens ?? 0;
      return sum + input + output;
    }, 0);
  }

  private sumCost(records: ResourceUsageRecord[]): number {
    return records.reduce((sum, r) => sum + (r.llmCost ?? 0), 0);
  }

  private calculateAvgIterationTime(records: ResourceUsageRecord[]): number {
    if (records.length === 0) return 0;

    const totalTime = records.reduce((sum, r) => {
      const toolTime = r.timingBreakdown?.toolExecutionTime ?? 0;
      const processingTime = r.timingBreakdown?.resultProcessingTime ?? 0;
      return sum + toolTime + processingTime;
    }, 0);

    return totalTime / records.length;
  }

  private calculatePeakMemory(records: ResourceUsageRecord[]): number {
    if (records.length === 0) return 0;
    return Math.max(...records.map((r) => r.memoryPeak ?? 0));
  }

  private breakdownToolExecution(records: ResourceUsageRecord[]): Record<string, number> {
    const breakdown: Record<string, number> = {};

    // Aggregate tool execution times across all records
    // Note: This requires tool information in ResourceUsageRecord
    // Current implementation provides a placeholder that can be enhanced
    // when tool-specific metrics are available in ResourceUsageRecord

    records.forEach((record) => {
      const toolTime = record.timingBreakdown?.toolExecutionTime ?? 0;
      if (toolTime > 0) {
        // Default key when specific tool info is not available
        const toolKey = "unspecified_tools";
        breakdown[toolKey] = (breakdown[toolKey] ?? 0) + toolTime;
      }
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
      target.averageIterationTime * target.timeline.length ||
      target.timeline.reduce(
        (sum, r) => sum + (r.timingBreakdown?.toolExecutionTime ?? 0),
        0,
      );
    const baselineTotalTime =
      baseline.averageIterationTime * baseline.timeline.length ||
      baseline.timeline.reduce(
        (sum, r) => sum + (r.timingBreakdown?.toolExecutionTime ?? 0),
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
