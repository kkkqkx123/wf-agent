/**
 * WorkflowPerformanceAnalysisAPI - Workflow Performance Analysis API
 *
 * Provides comprehensive performance tracking and analysis for Workflow executions.
 * Mirrors the Agent's AgentPerformanceAnalysisAPI for workflow context.
 *
 * Features:
 * - Execution timeline with detailed timing information
 * - Node-level performance tracking
 * - Performance bottleneck identification
 * - Node-by-node performance breakdown
 * - Cross-execution comparison
 *
 * @deprecated Use {@link PerformanceMetricsAPI} from "@sdk/api/shared/resources/metrics/performance-metrics-api.js" instead.
 * The `PerformanceMetricsAPI` now provides `analyzePerformance` and `getNodeComparison`
 * methods alongside its existing timeline and comparison capabilities.
 * This class is kept for backward compatibility and will be removed in a future major version.
 *
 * Part of the performance API unification effort to reduce duplication between
 * Agent and Workflow performance analysis implementations.
 */

import { QueryableResourceAPI } from "../../shared/resources/generic-resource-api.js";
import type { APIDependencyManager } from "../../shared/core/sdk-dependencies.js";
import type { ID } from "@wf-agent/types";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import type {
  OperationMetrics,
  PerformanceTier,
  PerformanceBottleneck,
  PerformanceSummary,
  PerformanceTrend,
  ExecutionTimelineEntry,
  ExecutionTimelineType,
} from "../../shared/resources/metrics/performance-metrics-api.js";

import {
  classifyPerformance,
  calculatePerformanceSummary,
  identifyBottlenecks,
} from "../../shared/utils/performance-utils.js";

const logger = createContextualLogger({ operation: "WorkflowPerformanceAnalysisAPI" });

// ============================================================================
// Type Definitions: Workflow Performance
// ============================================================================

/**
 * Workflow node execution performance record
 */
export interface NodeExecutionPerformance {
  /** Node ID */
  nodeId: string;
  /** Node name */
  nodeName: string;
  /** Node type */
  nodeType: string;
  /** Start time (ms) */
  startTime: number;
  /** End time (ms) */
  endTime: number;
  /** Duration (ms) */
  duration: number;
  /** Whether execution was successful */
  success: boolean;
  /** Tool calls made during this node execution */
  toolCallCount: number;
  /** Performance tier classification */
  performanceTier: PerformanceTier;
  /** Detailed operation metrics */
  operations: OperationMetrics[];
  /** Error message if failed */
  error?: string;
}

/**
 * Workflow execution performance profile
 */
export interface WorkflowExecutionPerformanceProfile {
  /** Execution ID */
  executionId: ID;
  /** Workflow ID */
  workflowId: string;
  /** Workflow name */
  workflowName: string;
  /** Execution status */
  status: string;
  /** Start time (ms) */
  startTime: number;
  /** End time (ms) */
  endTime: number;
  /** Total duration (ms) */
  totalDuration: number;
  /** Total number of nodes executed */
  totalNodes: number;
  /** Total number of tool calls */
  totalToolCalls: number;
  /** Overall performance tier */
  performanceTier: PerformanceTier;
  /** Node-level performance breakdown */
  nodeExecutions: NodeExecutionPerformance[];
  /** Identified bottlenecks */
  bottlenecks: PerformanceBottleneck[];
  /** Performance summary */
  summary: PerformanceSummary;
}

/**
 * Workflow node execution comparison
 */
export interface WorkflowNodeComparison {
  /** Execution ID */
  executionId: ID;
  /** Total nodes executed */
  totalNodes: number;
  /** Fastest node */
  fastestNode: { nodeId: string; nodeName: string; duration: number } | null;
  /** Slowest node */
  slowestNode: { nodeId: string; nodeName: string; duration: number } | null;
  /** Average node duration (ms) */
  averageDuration: number;
  /** Duration variance */
  variance: number;
  /** Performance trend across execution */
  trend: PerformanceTrend;
}

// Re-export types from shared location for consumer convenience
export type {
  OperationMetrics,
  IterationPerformance,
  PerformanceTier,
  PerformanceBottleneck,
  PerformanceSummary,
  PerformanceTrend,
  IterationComparison,
  ExecutionTimelineEntry,
  ExecutionTimelineType,
} from "../../shared/resources/metrics/performance-metrics-api.js";

// ============================================================================
// API Implementation
// ============================================================================

/**
 * Workflow Performance Analysis API
 *
 * Provides detailed performance tracking and optimization recommendations
 * for Workflow executions.
 */
export class WorkflowPerformanceAnalysisAPI extends QueryableResourceAPI<
  WorkflowExecutionPerformanceProfile,
  ID,
  { executionId?: ID; workflowId?: string; status?: string }
> {
  private deps: APIDependencyManager;
  private performanceCache: Map<ID, WorkflowExecutionPerformanceProfile> = new Map();

  constructor(deps: APIDependencyManager) {
    super();
    this.deps = deps;
  }

  /**
   * Get performance profile for an execution
   */
  protected override async getResource(id: ID): Promise<WorkflowExecutionPerformanceProfile | null> {
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
  protected override async getAllResources(): Promise<WorkflowExecutionPerformanceProfile[]> {
    const registry = this.deps.getWorkflowExecutionRegistry();
    const entities = registry.getAll();

    const profiles: WorkflowExecutionPerformanceProfile[] = [];
    for (const entity of entities) {
      const executionData = entity.getWorkflowExecutionData();
      const profile = await this.analyzePerformance(executionData.id);
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
    resources: WorkflowExecutionPerformanceProfile[],
    filter: { executionId?: ID; workflowId?: string; status?: string },
  ): WorkflowExecutionPerformanceProfile[] {
    let filtered = resources;

    if (filter.executionId) {
      filtered = filtered.filter(p => p.executionId === filter.executionId);
    }
    if (filter.workflowId) {
      filtered = filtered.filter(p => p.workflowId === filter.workflowId);
    }
    if (filter.status) {
      filtered = filtered.filter(p => p.status === filter.status);
    }

    return filtered;
  }

  /**
   * Analyze performance for a specific workflow execution
   */
  async analyzePerformance(executionId: ID): Promise<WorkflowExecutionPerformanceProfile | null> {
    logger.debug("Analyzing workflow performance", { executionId });

    try {
      const registry = this.deps.getWorkflowExecutionRegistry();
      const entity = registry.get(executionId);

      if (!entity) {
        logger.warn("Workflow execution not found", { executionId });
        return null;
      }

      const executionData = entity.getWorkflowExecutionData();
      const startTime = entity.getStartTime() || 0;
      const endTime = entity.getEndTime() || Date.now();
      const totalDuration = endTime - startTime;

      // Build node execution performance data
      const nodeExecutions: NodeExecutionPerformance[] = [];
      let totalToolCalls = 0;

      // Extract node execution records from the entity
      const nodeResults = entity.getNodeResults() || [];

      for (const nodeResult of nodeResults) {
        const nodeResultTyped = nodeResult as unknown as Record<string, unknown>;
        const nodeStartedAt = (nodeResultTyped["startedAt"] as number) || 0;
        const nodeCompletedAt = (nodeResultTyped["completedAt"] as number) || endTime;
        const nodeDuration = nodeCompletedAt - nodeStartedAt;

        // Build operation metrics for this node
        const operations: OperationMetrics[] = [
          {
            name: `Node ${nodeResult.nodeId}`,
            type: 'iteration',
            startTime: nodeStartedAt,
            endTime: nodeCompletedAt,
            duration: nodeDuration,
            success: !nodeResultTyped["error"],
          },
        ];

        // Add tool call operations if available
        const toolCalls = nodeResultTyped["toolCalls"] as
          Array<{ name: string; startTime: number; endTime?: number; error?: string; result?: unknown }> | undefined;
        if (toolCalls) {
          for (const toolCall of toolCalls) {
            operations.push({
              name: toolCall.name,
              type: 'tool_call',
              startTime: toolCall.startTime,
              endTime: toolCall.endTime || nodeCompletedAt,
              duration: (toolCall.endTime || nodeCompletedAt) - toolCall.startTime,
              success: !toolCall.error && toolCall.result !== undefined,
            });
          }
          totalToolCalls += toolCalls.length;
        }

        // Classify node performance
        const performanceTier = this.classifyPerformance(nodeDuration);

        nodeExecutions.push({
          nodeId: nodeResult.nodeId,
          nodeName: (nodeResultTyped["nodeName"] as string) || nodeResult.nodeId,
          nodeType: (nodeResultTyped["nodeType"] as string) || 'unknown',
          startTime: nodeStartedAt,
          endTime: nodeCompletedAt,
          duration: nodeDuration,
          toolCallCount: toolCalls?.length || 0,
          operations,
          success: !nodeResultTyped["error"],
          performanceTier,
          error: nodeResultTyped["error"] as string | undefined,
        });
      }

      // Calculate summary
      const summary = this.calculatePerformanceSummary(nodeExecutions, totalDuration);

      // Identify bottlenecks
      const bottlenecks = this.identifyBottlenecks(nodeExecutions, totalDuration);

      // Determine overall performance tier
      const overallTier = this.classifyPerformance(totalDuration);

      const profile: WorkflowExecutionPerformanceProfile = {
        executionId,
        workflowId: executionData.workflowId,
        workflowName: executionData.workflowId,
        status: entity.getStatus() as string,
        startTime,
        endTime,
        totalDuration,
        totalNodes: nodeExecutions.length,
        totalToolCalls,
        performanceTier: overallTier,
        nodeExecutions,
        bottlenecks,
        summary,
      };

      // Cache the profile
      this.performanceCache.set(executionId, profile);

      return profile;
    } catch (error) {
      logger.error("Failed to analyze workflow performance", { executionId, error });
      return null;
    }
  }

  /**
   * Get execution timeline with performance metrics
   */
  async getExecutionTimeline(executionId: ID): Promise<ExecutionTimelineEntry[]> {
    logger.debug("Building workflow execution timeline", { executionId });

    const profile = await this.analyzePerformance(executionId);
    if (!profile) {
      return [];
    }

    const timeline: ExecutionTimelineEntry[] = [];

    for (const nodeExec of profile.nodeExecutions) {
      timeline.push({
        timestamp: nodeExec.startTime,
        type: 'iteration_start' as ExecutionTimelineType,
        description: `Node ${nodeExec.nodeName} started`,
        duration: nodeExec.duration,
        performanceTier: nodeExec.performanceTier,
      });

      for (const operation of nodeExec.operations) {
        timeline.push({
          timestamp: operation.startTime,
          type: operation.type as ExecutionTimelineType,
          description: `${operation.name} completed in ${operation.duration}ms`,
          duration: operation.duration,
          success: operation.success,
        });
      }

      timeline.push({
        timestamp: nodeExec.endTime,
        type: 'iteration_end' as ExecutionTimelineType,
        description: `Node ${nodeExec.nodeName} completed in ${nodeExec.duration}ms`,
        duration: nodeExec.duration,
        performanceTier: nodeExec.performanceTier,
      });
    }

    return timeline.sort((a, b) => a.timestamp - b.timestamp);
  }

  /**
   * Get performance comparison between nodes of an execution
   */
  async getNodeComparison(executionId: ID): Promise<WorkflowNodeComparison> {
    const profile = await this.analyzePerformance(executionId);
    if (!profile || profile.nodeExecutions.length === 0) {
      return {
        executionId,
        totalNodes: 0,
        fastestNode: null,
        slowestNode: null,
        averageDuration: 0,
        variance: 0,
        trend: 'stable',
      };
    }

    const durations = profile.nodeExecutions.map(n => n.duration);
    const average = durations.reduce((a, b) => a + b, 0) / durations.length;

    const variance = durations.reduce((sum, d) => sum + Math.pow(d - average, 2), 0) / durations.length;

    const fastestIdx = durations.indexOf(Math.min(...durations));
    const slowestIdx = durations.indexOf(Math.max(...durations));

    // Determine trend: compare first half vs second half of nodes
    const first = durations.slice(0, Math.floor(durations.length / 2));
    const last = durations.slice(Math.floor(durations.length / 2));
    const firstAvg = first.length > 0 ? first.reduce((a, b) => a + b, 0) / first.length : 0;
    const lastAvg = last.length > 0 ? last.reduce((a, b) => a + b, 0) / last.length : 0;

    const trend: PerformanceTrend =
      lastAvg < firstAvg * 0.8 ? 'improving' : lastAvg > firstAvg * 1.2 ? 'degrading' : 'stable';

    return {
      executionId,
      totalNodes: profile.nodeExecutions.length,
      fastestNode: {
        nodeId: profile.nodeExecutions[fastestIdx]!.nodeId,
        nodeName: profile.nodeExecutions[fastestIdx]!.nodeName,
        duration: durations[fastestIdx]!,
      },
      slowestNode: {
        nodeId: profile.nodeExecutions[slowestIdx]!.nodeId,
        nodeName: profile.nodeExecutions[slowestIdx]!.nodeName,
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
    return classifyPerformance(duration);
  }

  /**
   * Calculate performance summary
   */
  private calculatePerformanceSummary(
    nodeExecutions: NodeExecutionPerformance[],
    totalDuration: number,
  ): PerformanceSummary {
    return calculatePerformanceSummary(nodeExecutions, totalDuration, "node");
  }

  /**
   * Identify performance bottlenecks
   */
  private identifyBottlenecks(
    nodeExecutions: NodeExecutionPerformance[],
    totalDuration: number,
  ): PerformanceBottleneck[] {
    return identifyBottlenecks(
      nodeExecutions,
      totalDuration,
      (nodeExec) => `${nodeExec.nodeName} (${nodeExec.nodeId})`,
      "node",
    );
  }
}