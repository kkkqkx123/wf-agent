/**
 * Agent Performance Analysis API
 *
 * Provides comprehensive performance tracking and analysis for Agent Loop executions.
 *
 * Features:
 * - Execution timeline with detailed timing information
 * - Operation-level progress tracking
 * - Performance bottleneck identification
 * - Iteration-by-iteration performance breakdown
 *
 * Part of P1 priority: Improve system observability with performance metrics
 */

import { QueryableResourceAPI } from "../../shared/resources/generic-resource-api.js";
import type { APIDependencyManager } from "@sdk/api/shared/core/sdk-dependencies.js";
import type { ID, AgentLoopStatus } from "@wf-agent/types";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ operation: "AgentPerformanceAnalysisAPI" });

// ============================================================================
// Type Definitions: Performance Metrics
// ============================================================================

/**
 * Performance metrics for a single operation
 */
export interface OperationMetrics {
  /** Operation name */
  name: string;
  /** Operation type (e.g., 'llm_request', 'tool_call', 'iteration') */
  type: string;
  /** Start timestamp */
  startTime: number;
  /** End timestamp */
  endTime: number;
  /** Duration in milliseconds */
  duration: number;
  /** Whether operation succeeded */
  success: boolean;
  /** Resource usage estimates */
  resources?: {
    /** Estimated CPU time (ms) */
    cpuTime?: number;
    /** Estimated memory used (bytes) */
    memoryUsed?: number;
  };
}

/**
 * Iteration performance breakdown
 */
export interface IterationPerformance {
  /** Iteration number */
  iteration: number;
  /** Iteration start time */
  startTime: number;
  /** Iteration end time */
  endTime: number;
  /** Total iteration duration */
  duration: number;
  /** Number of tool calls in this iteration */
  toolCallCount: number;
  /** LLM request count */
  llmRequestCount: number;
  /** Detailed operation metrics */
  operations: OperationMetrics[];
  /** Whether iteration succeeded */
  success: boolean;
  /** Performance tier: 'fast' | 'normal' | 'slow' */
  performanceTier: PerformanceTier;
}

/**
 * Complete execution performance profile
 */
export interface ExecutionPerformanceProfile {
  /** Execution ID */
  executionId: ID;
  /** Execution status */
  status: AgentLoopStatus;
  /** Total execution start time */
  startTime: number;
  /** Total execution end time */
  endTime?: number;
  /** Total execution duration */
  totalDuration?: number;
  /** Total number of iterations */
  totalIterations: number;
  /** Total tool calls */
  totalToolCalls: number;
  /** Performance tier */
  performanceTier: PerformanceTier;
  /** Iteration-by-iteration breakdown */
  iterations: IterationPerformance[];
  /** Performance bottlenecks */
  bottlenecks: PerformanceBottleneck[];
  /** Performance summary */
  summary: PerformanceSummary;
}

/**
 * Performance tier classification
 */
export type PerformanceTier = 'fast' | 'normal' | 'slow';

/**
 * Performance bottleneck
 */
export interface PerformanceBottleneck {
  /** Bottleneck type */
  type: 'iteration' | 'tool_call' | 'llm_request';
  /** Location (iteration number or tool name) */
  location: string | number;
  /** Duration (ms) */
  duration: number;
  /** Percentage of total execution time */
  percentage: number;
  /** Severity: 'low' | 'medium' | 'high' */
  severity: 'low' | 'medium' | 'high';
}

/**
 * Performance summary
 */
export interface PerformanceSummary {
  /** Average iteration duration */
  avgIterationDuration: number;
  /** Fastest iteration duration */
  minIterationDuration: number;
  /** Slowest iteration duration */
  maxIterationDuration: number;
  /** Average tool call duration */
  avgToolCallDuration?: number;
  /** Success rate */
  successRate: number;
  /** Estimated operations per second */
  operationsPerSecond: number;
  /** Recommendations for optimization */
  recommendations: string[];
}

// ============================================================================
// API Implementation
// ============================================================================

/**
 * Agent Performance Analysis API
 *
 * Provides detailed performance tracking and optimization recommendations
 * for Agent Loop executions.
 */
export class AgentPerformanceAnalysisAPI extends QueryableResourceAPI<
  ExecutionPerformanceProfile,
  ID,
  { executionId?: ID; status?: string }
> {
  private deps: APIDependencyManager;
  private performanceCache: Map<ID, ExecutionPerformanceProfile> = new Map();

  constructor(deps: APIDependencyManager) {
    super();
    this.deps = deps;
  }

  /**
   * Get performance profile for an execution
   */
  protected override async getResource(id: ID): Promise<ExecutionPerformanceProfile | null> {
    // Try cache first
    if (this.performanceCache.has(id)) {
      return this.performanceCache.get(id)!;
    }

    // Otherwise, analyze from execution state
    return this.analyzePerformance(id);
  }

  /**
   * Get all performance profiles
   */
  protected override async getAllResources(): Promise<ExecutionPerformanceProfile[]> {
    const registry = this.deps.getAgentLoopRegistry();
    const entities = await registry.getAll();

    const profiles: ExecutionPerformanceProfile[] = [];
    for (const entity of entities) {
      const profile = await this.analyzePerformance(entity.id);
      if (profile) {
        profiles.push(profile);
      }
    }

    return profiles;
  }

  /**
   * Apply filter to performance profiles
   */
  protected override applyFilter(
    resources: ExecutionPerformanceProfile[],
    filter: { executionId?: ID; status?: string },
  ): ExecutionPerformanceProfile[] {
    let filtered = resources;

    if (filter.executionId) {
      filtered = filtered.filter(p => p.executionId === filter.executionId);
    }

    if (filter.status) {
      filtered = filtered.filter(p => p.status === filter.status);
    }

    return filtered;
  }

  /**
   * Analyze performance for a specific execution
   */
  async analyzePerformance(executionId: ID): Promise<ExecutionPerformanceProfile | null> {
    logger.debug("Analyzing performance", { executionId });

    try {
      const registry = this.deps.getAgentLoopRegistry();
      const entity = await registry.get(executionId);

      if (!entity) {
        logger.warn("Execution not found", { executionId });
        return null;
      }

      const state = entity.state;
      const startTime = state.startTime || 0;
      const endTime = state.endTime || Date.now();
      const totalDuration = endTime - startTime;

      // Build iteration performance data
      const iterations: IterationPerformance[] = [];
      let totalToolCalls = 0;

      for (const record of state.iterationHistory) {
        const iterationDuration = (record.endTime || endTime) - record.startTime;

        // Build operation metrics for this iteration
        const operations: OperationMetrics[] = [
          {
            name: `Iteration ${record.iteration}`,
            type: 'iteration',
            startTime: record.startTime,
            endTime: record.endTime || endTime,
            duration: iterationDuration,
            success: true,
          },
        ];

        // Add tool call operations
        for (const toolCall of record.toolCalls) {
          operations.push({
            name: toolCall.name,
            type: 'tool_call',
            startTime: toolCall.startTime,
            endTime: toolCall.endTime || record.endTime || endTime,
            duration: (toolCall.endTime || record.endTime || endTime) - toolCall.startTime,
            success: !toolCall.error && toolCall.result !== undefined,
          });
        }

        totalToolCalls += record.toolCalls.length;

        // Classify iteration performance
        const performanceTier = this.classifyPerformance(iterationDuration);

        iterations.push({
          iteration: record.iteration,
          startTime: record.startTime,
          endTime: record.endTime || endTime,
          duration: iterationDuration,
          toolCallCount: record.toolCalls.length,
          llmRequestCount: 1, // Assuming one LLM request per iteration
          operations,
          success: record.endTime !== null,
          performanceTier,
        });
      }

      // Calculate summary
      const summary = this.calculatePerformanceSummary(iterations, totalDuration);

      // Identify bottlenecks
      const bottlenecks = this.identifyBottlenecks(iterations, totalDuration);

      // Determine overall performance tier
      const overallTier = this.classifyPerformance(totalDuration);

      const profile: ExecutionPerformanceProfile = {
        executionId,
        status: entity.getStatus(),
        startTime,
        endTime,
        totalDuration,
        totalIterations: state.currentIteration,
        totalToolCalls,
        performanceTier: overallTier,
        iterations,
        bottlenecks,
        summary,
      };

      // Cache the profile
      this.performanceCache.set(executionId, profile);

      return profile;
    } catch (error) {
      logger.error("Failed to analyze performance", { executionId, error });
      return null;
    }
  }

  /**
   * Get execution timeline with performance metrics
   */
  async getExecutionTimeline(executionId: ID): Promise<ExecutionTimelineEntry[]> {
    logger.debug("Building execution timeline", { executionId });

    const profile = await this.analyzePerformance(executionId);
    if (!profile) {
      return [];
    }

    const timeline: ExecutionTimelineEntry[] = [];

    for (const iteration of profile.iterations) {
      timeline.push({
        timestamp: iteration.startTime,
        type: 'iteration_start',
        description: `Iteration ${iteration.iteration} started`,
        duration: iteration.duration,
        performanceTier: iteration.performanceTier,
      });

      for (const operation of iteration.operations) {
        timeline.push({
          timestamp: operation.startTime,
          type: operation.type as ExecutionTimelineType,
          description: `${operation.name} completed in ${operation.duration}ms`,
          duration: operation.duration,
          success: operation.success,
        });
      }

      timeline.push({
        timestamp: iteration.endTime,
        type: 'iteration_end',
        description: `Iteration ${iteration.iteration} completed in ${iteration.duration}ms`,
        duration: iteration.duration,
        performanceTier: iteration.performanceTier,
      });
    }

    return timeline.sort((a, b) => a.timestamp - b.timestamp);
  }

  /**
   * Get performance comparison between iterations
   */
  async getIterationComparison(executionId: ID): Promise<IterationComparison> {
    const profile = await this.analyzePerformance(executionId);
    if (!profile || profile.iterations.length === 0) {
      return {
        executionId,
        totalIterations: 0,
        fastestIteration: null,
        slowestIteration: null,
        averageDuration: 0,
        variance: 0,
        trend: 'stable',
      };
    }

    const durations = profile.iterations.map(i => i.duration);
    const average = durations.reduce((a, b) => a + b, 0) / durations.length;

    const variance = durations.reduce((sum, d) => sum + Math.pow(d - average, 2), 0) / durations.length;

    const fastestIdx = durations.indexOf(Math.min(...durations));
    const slowestIdx = durations.indexOf(Math.max(...durations));

    // Determine trend
    const first = durations.slice(0, Math.floor(durations.length / 2));
    const last = durations.slice(Math.floor(durations.length / 2));
    const firstAvg = first.reduce((a, b) => a + b, 0) / first.length;
    const lastAvg = last.reduce((a, b) => a + b, 0) / last.length;

    const trend: PerformanceTrend =
      lastAvg < firstAvg * 0.8 ? 'improving' : lastAvg > firstAvg * 1.2 ? 'degrading' : 'stable';

    return {
      executionId,
      totalIterations: profile.iterations.length,
      fastestIteration: {
        iteration: profile.iterations[fastestIdx]!.iteration,
        duration: durations[fastestIdx]!,
      },
      slowestIteration: {
        iteration: profile.iterations[slowestIdx]!.iteration,
        duration: durations[slowestIdx]!,
      },
      averageDuration: Math.round(average),
      variance: Math.round(variance),
      trend,
    };
  }

  /**
   * Classify performance into tiers
   */
  private classifyPerformance(duration: number): PerformanceTier {
    // Thresholds can be configured per use case
    const fastThreshold = 1000; // 1 second
    const normalThreshold = 5000; // 5 seconds

    if (duration < fastThreshold) {
      return 'fast';
    } else if (duration < normalThreshold) {
      return 'normal';
    } else {
      return 'slow';
    }
  }

  /**
   * Calculate performance summary
   */
  private calculatePerformanceSummary(
    iterations: IterationPerformance[],
    totalDuration: number,
  ): PerformanceSummary {
    if (iterations.length === 0) {
      return {
        avgIterationDuration: 0,
        minIterationDuration: 0,
        maxIterationDuration: 0,
        successRate: 0,
        operationsPerSecond: 0,
        recommendations: [],
      };
    }

    const durations = iterations.map(i => i.duration);
    const avgDuration = Math.round(durations.reduce((a, b) => a + b, 0) / durations.length);
    const minDuration = Math.min(...durations);
    const maxDuration = Math.max(...durations);

    const successCount = iterations.filter(i => i.success).length;
    const successRate = (successCount / iterations.length) * 100;

    const totalToolCalls = iterations.reduce((sum, i) => sum + i.toolCallCount, 0);
    const operationsPerSecond = Math.round((totalToolCalls / totalDuration) * 1000);

    // Generate recommendations
    const recommendations: string[] = [];

    if (maxDuration / avgDuration > 2) {
      recommendations.push('High variance in iteration duration detected. Investigate slow iterations.');
    }

    if (operationsPerSecond < 1) {
      recommendations.push('Low throughput. Consider optimizing tool execution.');
    }

    if (successRate < 90) {
      recommendations.push('High failure rate. Review error logs for patterns.');
    }

    return {
      avgIterationDuration: avgDuration,
      minIterationDuration: minDuration,
      maxIterationDuration: maxDuration,
      avgToolCallDuration: Math.round(totalDuration / totalToolCalls),
      successRate: Math.round(successRate * 100) / 100,
      operationsPerSecond,
      recommendations,
    };
  }

  /**
   * Identify performance bottlenecks
   */
  private identifyBottlenecks(
    iterations: IterationPerformance[],
    totalDuration: number,
  ): PerformanceBottleneck[] {
    const bottlenecks: PerformanceBottleneck[] = [];

    // Find slow iterations
    const iterationDurations = iterations.map(i => i.duration);
    const avgIterationDuration =
      iterationDurations.reduce((a, b) => a + b, 0) / iterationDurations.length;

    for (const iteration of iterations) {
      if (iteration.duration > avgIterationDuration * 1.5) {
        bottlenecks.push({
          type: 'iteration',
          location: iteration.iteration,
          duration: iteration.duration,
          percentage: (iteration.duration / totalDuration) * 100,
          severity: iteration.duration > avgIterationDuration * 2.5 ? 'high' : 'medium',
        });
      }

      // Find slow tool calls
      for (const operation of iteration.operations) {
        if (operation.type === 'tool_call' && operation.duration > 1000) {
          bottlenecks.push({
            type: 'tool_call',
            location: operation.name,
            duration: operation.duration,
            percentage: (operation.duration / totalDuration) * 100,
            severity: operation.duration > 5000 ? 'high' : 'medium',
          });
        }
      }
    }

    return bottlenecks.sort((a, b) => b.duration - a.duration).slice(0, 10);
  }
}

// ============================================================================
// Additional Type Definitions
// ============================================================================

/**
 * Execution timeline entry
 */
export interface ExecutionTimelineEntry {
  /** Event timestamp */
  timestamp: number;
  /** Event type */
  type: ExecutionTimelineType;
  /** Event description */
  description: string;
  /** Event duration */
  duration?: number;
  /** Performance tier */
  performanceTier?: PerformanceTier;
  /** Whether event succeeded */
  success?: boolean;
}

/**
 * Timeline entry type
 */
export type ExecutionTimelineType = 'iteration_start' | 'iteration_end' | 'tool_call' | 'llm_request';

/**
 * Performance trend
 */
export type PerformanceTrend = 'improving' | 'degrading' | 'stable';

/**
 * Iteration comparison result
 */
export interface IterationComparison {
  /** Execution ID */
  executionId: ID;
  /** Total iterations */
  totalIterations: number;
  /** Fastest iteration */
  fastestIteration: {
    iteration: number;
    duration: number;
  } | null;
  /** Slowest iteration */
  slowestIteration: {
    iteration: number;
    duration: number;
  } | null;
  /** Average iteration duration */
  averageDuration: number;
  /** Variance in durations */
  variance: number;
  /** Performance trend */
  trend: PerformanceTrend;
}
