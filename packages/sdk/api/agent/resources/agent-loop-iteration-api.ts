/**
 * AgentLoopIterationAPI - Agent Loop Iteration History & Analysis API
 * Provides comprehensive iteration metadata, analysis, and tracking capabilities
 *
 * Consolidates:
 * - Extended iteration detail query with comprehensive metadata
 * - Decision pattern analysis and reasoning chains
 * - Resource consumption tracking and optimization opportunities
 * - Execution flow visualization data
 */

import { QueryableResourceAPI } from "../../shared/resources/generic-resource-api.js";
import type { APIDependencyManager } from "@sdk/api/shared/core/sdk-dependencies.js";
import type { ID, IterationRecord } from "@wf-agent/types";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ operation: "AgentLoopIterationAPI" });

// ============================================================================
// Type Definitions: Decision Tracking
// ============================================================================

/**
 * Decision Outcome Record
 * Records conditional logic decisions made by LLM
 */
export interface DecisionOutcome {
  /** Decision identifier */
  id: string;
  /** Decision description (e.g., "Choose route A vs B") */
  description: string;
  /** Selected branch/option */
  selectedOption: string;
  /** All available options */
  availableOptions: string[];
  /** Reasoning behind the decision */
  reasoning?: string;
  /** Confidence level (0-1) */
  confidence?: number;
}

/**
 * Tool Dependency Record
 * Represents dependency relationships between tool calls
 */
export interface ToolDependency {
  /** Tool call ID */
  toolCallId: string;
  /** Tool name */
  toolName: string;
  /** IDs of tools this call depends on */
  dependsOn: string[];
  /** Dependency type */
  dependencyType: "sequential" | "parallel" | "conditional" | "result-dependent";
  /** Why this dependency exists */
  reason?: string;
}

/**
 * Execution Path Record
 * Tracks the control flow and branching decisions
 */
export interface ExecutionPath {
  /** Unique path identifier */
  pathId: string;
  /** Path description */
  description: string;
  /** Sequence of steps/decisions taken */
  steps: Array<{
    stepId: string;
    type: "decision" | "action" | "tool_call" | "branch";
    description: string;
    result?: unknown;
    timestamp: number;
  }>;
  /** Whether this path was optimal */
  isOptimal?: boolean;
  /** Alternative paths that were considered */
  alternativePaths?: string[];
}

/**
 * LLM Reasoning Record
 * Captures the reasoning process of the LLM
 */
export interface LLMReasoningRecord {
  /** Reasoning step identifier */
  stepId: string;
  /** Type of reasoning (thinking, planning, analyzing, evaluating) */
  reasoningType: "thinking" | "planning" | "analyzing" | "evaluating" | "synthesizing";
  /** Reasoning content */
  content: string;
  /** Confidence in this reasoning */
  confidence?: number;
  /** Related entities (e.g., tools, data sources) */
  relatedEntities?: string[];
  /** Conclusions drawn */
  conclusions?: string[];
}

/**
 * Error Context Record
 * Provides detailed error handling context
 */
export interface ErrorContextRecord {
  /** Error identifier */
  errorId: string;
  /** Error type */
  errorType: "tool_error" | "validation_error" | "execution_error" | "timeout" | "other";
  /** Error message */
  errorMessage: string;
  /** Tool call ID if error occurred during tool execution */
  toolCallId?: string;
  /** Recovery action taken */
  recoveryAction?: string;
  /** Whether recovery was successful */
  recoverySuccess?: boolean;
}

/**
 * System Metrics Record
 * Tracks infrastructure costs and system resource consumption during iteration
 * Persisted separately from LLM metrics for independent query patterns
 */
export interface IterationSystemMetrics {
  /** Iteration number */
  iteration: number;
  /** Timestamp (milliseconds since epoch) */
  timestamp: number;
  /** CPU time used (milliseconds) */
  cpuTimeMs: number;
  /** Memory peak usage (megabytes) */
  memoryPeakMb: number;
  /** Total duration (milliseconds) */
  durationMs: number;
  /** Disk I/O bytes */
  diskIoBytes?: number;
  /** Network I/O bytes */
  networkIoBytes?: number;
}

/**
 * LLM Metrics Record
 * Tracks model consumption and API costs during iteration
 * Persisted separately from system metrics for cost analysis
 */
export interface IterationLLMMetrics {
  /** Iteration number */
  iteration: number;
  /** Associated tool call ID (if from a specific tool call) */
  toolCallId?: string;
  /** Timestamp (milliseconds since epoch) */
  timestamp: number;
  /** Input tokens consumed */
  inputTokens: number;
  /** Output tokens generated */
  outputTokens: number;
  /** Cost in USD */
  costUsd: number;
  /** Model name/identifier */
  model: string;
  /** Request duration (milliseconds) */
  durationMs: number;
  /** Cache read tokens (for models supporting prompt caching) */
  cacheReadTokens?: number;
  /** Cache creation tokens (for models supporting prompt caching) */
  cacheCreationTokens?: number;
}

/**
 * Extended Iteration Detail
 *
 * Enhances the base IterationRecord with comprehensive metadata
 * for advanced execution analysis, debugging, and optimization.
 */
export interface ExtendedIterationDetail extends IterationRecord {
  /** Agent Loop ID this iteration belongs to */
  agentLoopId: string;

  // ============================================================================
  // Execution Flow & Decision Tracking
  // ============================================================================

  /** Decision outcomes during this iteration */
  decisions?: DecisionOutcome[];

  /** Tool execution dependencies */
  toolDependencies?: ToolDependency[];

  /** Execution path taken */
  executionPath?: ExecutionPath;

  // ============================================================================
  // LLM Reasoning & Analysis
  // ============================================================================

  /** LLM reasoning steps */
  reasoningSteps?: LLMReasoningRecord[];

  /** LLM thinking process (raw thoughts if available) */
  llmThinkingProcess?: string;

  /** Prompt used for this iteration */
  prompt?: string;

  /** LLM model used */
  llmModel?: string;

  /** LLM temperature/parameters used */
  llmParameters?: Record<string, unknown>;

  // ============================================================================
  // Error Handling & Recovery
  // ============================================================================

  /** Error contexts during this iteration */
  errors?: ErrorContextRecord[];

  /** Whether iteration had recoverable errors */
  hadRecoverableErrors?: boolean;

  /** Retry count */
  retryCount?: number;

  // ============================================================================
  // Resource Consumption
  // ============================================================================

  /** System metrics for this iteration */
  systemMetrics?: IterationSystemMetrics;

  /** LLM metrics for this iteration */
  llmMetrics?: IterationLLMMetrics[];

  // ============================================================================
  // Quality & Optimization Metrics
  // ============================================================================

  /** Quality score for this iteration (0-1) */
  qualityScore?: number;

  /** Whether iteration output was approved without revision */
  requiresRevision?: boolean;

  /** Optimization opportunities identified */
  optimizationOpportunities?: string[];

  /** Iteration tags for categorization */
  tags?: string[];

  // ============================================================================
  // Audit & Compliance
  // ============================================================================

  /** Audit log entries */
  auditLog?: Array<{
    timestamp: number;
    action: string;
    details?: unknown;
  }>;

  /** Compliance check results */
  complianceChecks?: Record<string, boolean>;
}

/**
 * Extended Iteration History Summary
 *
 * Provides enhanced statistics with additional metadata
 */
export interface ExtendedIterationHistorySummary {
  /** Total number of iterations */
  totalIterations: number;

  /** Total tool calls across all iterations */
  totalToolCalls: number;

  /** Total elapsed time across all iterations (ms) */
  totalDuration: number;

  /** Average duration per iteration (ms) */
  averageDuration: number;

  /** Agent loop status */
  status: string;

  // ============================================================================
  // Enhanced Metrics
  // ============================================================================

  /** Total decision points */
  totalDecisions?: number;

  /** Average quality score */
  averageQualityScore?: number;

  /** Iterations requiring revision */
  revisionsRequired?: number;

  /** Total errors encountered */
  totalErrors?: number;

  /** Error rate (0-1) */
  errorRate?: number;

  /** Total recovery actions */
  totalRecoveryActions?: number;

  /** Recovery success rate (0-1) */
  recoverySuccessRate?: number;

  /** Tool dependency graph complexity (0-1) */
  dependencyComplexity?: number;

  // ============================================================================
  // Resource Metrics
  // ============================================================================

  /** Total LLM tokens used */
  totalLLMTokens?: number;

  /** Average tokens per iteration */
  averageLLMTokensPerIteration?: number;

  /** Peak memory usage (bytes) */
  peakMemoryUsage?: number;

  /** Total API calls made */
  totalAPICalls?: number;

  // ============================================================================
  // Optimization Summary
  // ============================================================================

  /** Most common optimization opportunities */
  topOptimizations?: Array<{
    opportunity: string;
    frequency: number;
    estimatedImpact?: string;
  }>;

  /** Slowest tools identified */
  slowestTools?: Array<{
    toolName: string;
    averageTime: number;
  }>;

  /** Most frequently called tools */
  frequentTools?: Array<{
    toolName: string;
    count: number;
  }>;
}

// ============================================================================
// Query & Analysis Interfaces
// ============================================================================

/**
 * Extended Iteration Filter
 */
export interface ExtendedIterationFilter {
  /** Agent Loop ID list */
  agentLoopIds?: ID[];
  /** Iteration number range */
  iterationRange?: {
    start?: number;
    end?: number;
  };
  /** Filter by quality score */
  minQualityScore?: number;
  /** Filter iterations with errors */
  hasErrors?: boolean;
  /** Filter iterations requiring revision */
  requiresRevision?: boolean;
  /** Filter by tags */
  tags?: string[];
  /** Time range */
  timeRange?: {
    start?: number;
    end?: number;
  };
}

/**
 * Decision Analysis Result
 */
export interface DecisionAnalysis {
  /** Total decision points */
  totalDecisions: number;
  /** Decision types distribution */
  decisionTypes: Record<string, number>;
  /** Average decision confidence */
  averageConfidence: number;
  /** Most common decisions */
  frequentDecisions: Array<{
    description: string;
    frequency: number;
  }>;
  /** Decision reversal count */
  reversalCount: number;
}

/**
 * Execution Path Analysis Result
 */
export interface ExecutionPathAnalysis {
  /** Total unique paths */
  totalPaths: number;
  /** Average path length */
  averagePathLength: number;
  /** Most common path */
  mostCommonPath?: string;
  /** Path complexity score (0-1) */
  complexityScore: number;
  /** Paths marked as optimal */
  optimalPathCount: number;
}

// ============================================================================
// API Implementation
// ============================================================================

/**
 * AgentLoopIterationAPI - Agent Loop Iteration History & Analysis API
 */
export class AgentLoopIterationAPI extends QueryableResourceAPI<
  ExtendedIterationDetail,
  string,
  ExtendedIterationFilter
> {
  private iterationDetails: Map<string, ExtendedIterationDetail> = new Map();

  /**
   * Constructor
   * @param deps APIDependencyManager instance
   */
  constructor(deps: APIDependencyManager) {
    super();
    void deps; // Acknowledge parameter
  }

  // ============================================================================
  // Implementing Abstract Methods
  // ============================================================================

  /**
   * Get extended iteration detail by ID
   * @param id Iteration detail ID
   * @returns Extended iteration detail or null
   */
  protected override async getResource(id: string): Promise<ExtendedIterationDetail | null> {
    return this.iterationDetails.get(id) ?? null;
  }

  /**
   * Get all extended iteration details
   * @returns Array of extended iteration details
   */
  protected override async getAllResources(): Promise<ExtendedIterationDetail[]> {
    return Array.from(this.iterationDetails.values());
  }

  /**
   * Apply filters to iteration details
   * @param records Iteration details
   * @param filter Query filter
   * @returns Filtered records
   */
  protected override applyFilter(
    records: ExtendedIterationDetail[],
    filter: ExtendedIterationFilter,
  ): ExtendedIterationDetail[] {
    let filtered = records;

    if (filter.agentLoopIds && filter.agentLoopIds.length > 0) {
      const idSet = new Set(filter.agentLoopIds);
      filtered = filtered.filter(r => idSet.has(r.agentLoopId));
    }

    if (filter.iterationRange) {
      const { start, end } = filter.iterationRange;
      if (start !== undefined) {
        filtered = filtered.filter(r => r.iteration >= start);
      }
      if (end !== undefined) {
        filtered = filtered.filter(r => r.iteration <= end);
      }
    }

    if (filter.minQualityScore !== undefined) {
      filtered = filtered.filter(r => (r.qualityScore ?? 0) >= filter.minQualityScore!);
    }

    if (filter.hasErrors) {
      filtered = filtered.filter(r => r.errors && r.errors.length > 0);
    }

    if (filter.requiresRevision) {
      filtered = filtered.filter(r => r.requiresRevision === true);
    }

    if (filter.tags && filter.tags.length > 0) {
      const tagSet = new Set(filter.tags);
      filtered = filtered.filter(r => r.tags && r.tags.some(t => tagSet.has(t)));
    }

    if (filter.timeRange) {
      const { start, end } = filter.timeRange;
      if (start !== undefined) {
        filtered = filtered.filter(r => r.startTime >= start);
      }
      if (end !== undefined) {
        filtered = filtered.filter(r => (r.endTime ?? 0) <= end);
      }
    }

    return filtered;
  }

  /**
   * Record extended iteration detail
   * @param detail Extended iteration detail
   */
  async recordIterationDetail(detail: ExtendedIterationDetail): Promise<void> {
    const id = `${detail.agentLoopId}:iteration:${detail.iteration}`;
    this.iterationDetails.set(id, detail);
    logger.debug("Recorded extended iteration detail", {
      iterationId: id,
      agentLoopId: detail.agentLoopId,
      iteration: detail.iteration,
    });
  }

  /**
   * Get extended iteration history summary
   * @param agentLoopId Agent Loop ID
   * @returns Extended history summary
   */
  async getExtendedHistorySummary(agentLoopId: ID): Promise<ExtendedIterationHistorySummary> {
    const records = Array.from(this.iterationDetails.values()).filter(r => r.agentLoopId === agentLoopId);

    const summary: ExtendedIterationHistorySummary = {
      totalIterations: records.length,
      totalToolCalls: records.reduce((sum, r) => sum + (r.toolCalls?.length ?? 0), 0),
      totalDuration: records.reduce((sum, r) => sum + ((r.endTime ?? r.startTime) - r.startTime), 0),
      averageDuration: 0,
      status: "RUNNING",
      totalDecisions: 0,
      averageQualityScore: 0,
      revisionsRequired: 0,
      totalErrors: 0,
      errorRate: 0,
      totalRecoveryActions: 0,
      recoverySuccessRate: 0,
      dependencyComplexity: 0,
      totalLLMTokens: 0,
      averageLLMTokensPerIteration: 0,
      peakMemoryUsage: 0,
      totalAPICalls: 0,
      topOptimizations: [],
      slowestTools: [],
      frequentTools: [],
    };

    if (records.length === 0) {
      return summary;
    }

    summary.averageDuration = summary.totalDuration / records.length;

    // Calculate enhanced metrics
    let qualityScoreSum = 0;
    let decisionSum = 0;
    let errorSum = 0;
    let recoverySum = 0;
    let recoverySuccessSum = 0;
    let tokenSum = 0;
    let maxMemory = 0;
    let apiCallSum = 0;
    const toolFrequency: Record<string, number> = {};
    const toolTiming: Record<string, number[]> = {};
    const optimizations: Record<string, number> = {};

    records.forEach(record => {
      if (record.qualityScore) qualityScoreSum += record.qualityScore;
      if (record.decisions) decisionSum += record.decisions.length;
      if (record.errors) errorSum += record.errors.length;
      if (record.requiresRevision) summary.revisionsRequired!++;
      if (record.errors && record.errors.some(e => e.recoveryAction)) recoverySum++;
      // Migrate from legacy resourceUsage to new separated metrics
      if (record.llmMetrics) {
        record.llmMetrics.forEach(metric => {
          tokenSum += metric.inputTokens + metric.outputTokens;
        });
      }
      if (record.systemMetrics) {
        maxMemory = Math.max(maxMemory, record.systemMetrics.memoryPeakMb);
      }

      // Track tool usage
      record.toolCalls?.forEach(tool => {
        toolFrequency[tool.name] = (toolFrequency[tool.name] || 0) + 1;
        if (!toolTiming[tool.name]) {
          toolTiming[tool.name] = [];
        }
        const duration = (tool.endTime ?? tool.startTime) - tool.startTime;
        const timings = toolTiming[tool.name];
        if (timings) {
          timings.push(duration);
        }
      });

      // Track optimizations
      record.optimizationOpportunities?.forEach(opt => {
        optimizations[opt] = (optimizations[opt] || 0) + 1;
      });
    });

    summary.totalDecisions = decisionSum;
    summary.averageQualityScore = records.length > 0 ? qualityScoreSum / records.length : 0;
    summary.totalErrors = errorSum;
    summary.errorRate = records.length > 0 ? errorSum / records.length : 0;
    summary.totalRecoveryActions = recoverySum;
    summary.recoverySuccessRate = recoverySum > 0 ? recoverySuccessSum / recoverySum : 0;
    summary.totalLLMTokens = tokenSum;
    summary.averageLLMTokensPerIteration = records.length > 0 ? tokenSum / records.length : 0;
    summary.peakMemoryUsage = maxMemory;
    summary.totalAPICalls = apiCallSum;

    // Find slowest tools
    summary.slowestTools = Object.entries(toolTiming)
      .map(([toolName, timings]) => ({
        toolName,
        averageTime: timings.reduce((a, b) => a + b, 0) / timings.length,
      }))
      .sort((a, b) => b.averageTime - a.averageTime)
      .slice(0, 5);

    // Find frequent tools
    summary.frequentTools = Object.entries(toolFrequency)
      .map(([toolName, count]) => ({ toolName, count }))
      .sort((a, b) => b.count - a.count)
      .slice(0, 5);

    // Find top optimizations
    summary.topOptimizations = Object.entries(optimizations)
      .map(([opportunity, frequency]) => ({ opportunity, frequency, estimatedImpact: "Unknown" }))
      .sort((a, b) => b.frequency - a.frequency)
      .slice(0, 5);

    return summary;
  }

  /**
   * Analyze decision patterns
   * @param agentLoopId Agent Loop ID
   * @returns Decision analysis
   */
  async analyzeDecisions(agentLoopId: ID): Promise<DecisionAnalysis> {
    const records = Array.from(this.iterationDetails.values()).filter(r => r.agentLoopId === agentLoopId);
    const allDecisions = records.flatMap(r => r.decisions ?? []);

    const analysis: DecisionAnalysis = {
      totalDecisions: allDecisions.length,
      decisionTypes: {},
      averageConfidence: 0,
      frequentDecisions: [],
      reversalCount: 0,
    };

    if (allDecisions.length === 0) {
      return analysis;
    }

    // Analyze decision types
    allDecisions.forEach(decision => {
      const parts = decision.description.split(" ");
      const key = parts[0] || "unknown";
      analysis.decisionTypes[key] = (analysis.decisionTypes[key] || 0) + 1;
    });

    // Calculate average confidence
    const confidences = allDecisions.filter(d => d.confidence !== undefined).map(d => d.confidence!);
    if (confidences.length > 0) {
      analysis.averageConfidence = confidences.reduce((a, b) => a + b, 0) / confidences.length;
    }

    // Find frequent decisions
    const decisionFrequency: Record<string, number> = {};
    allDecisions.forEach(d => {
      decisionFrequency[d.description] = (decisionFrequency[d.description] || 0) + 1;
    });
    analysis.frequentDecisions = Object.entries(decisionFrequency)
      .map(([description, frequency]) => ({ description, frequency }))
      .sort((a, b) => b.frequency - a.frequency)
      .slice(0, 5);

    return analysis;
  }

  /**
   * Analyze execution paths
   * @param agentLoopId Agent Loop ID
   * @returns Path analysis
   */
  async analyzePaths(agentLoopId: ID): Promise<ExecutionPathAnalysis> {
    const records = Array.from(this.iterationDetails.values()).filter(r => r.agentLoopId === agentLoopId);
    const paths = records.filter(r => r.executionPath).map(r => r.executionPath!);

    const analysis: ExecutionPathAnalysis = {
      totalPaths: new Set(paths.map(p => p.pathId)).size,
      averagePathLength: 0,
      mostCommonPath: undefined,
      complexityScore: 0,
      optimalPathCount: 0,
    };

    if (paths.length === 0) {
      return analysis;
    }

    // Calculate average path length
    const pathLengths = paths.map(p => p.steps.length);
    analysis.averagePathLength = pathLengths.reduce((a, b) => a + b, 0) / pathLengths.length;

    // Count optimal paths
    analysis.optimalPathCount = paths.filter(p => p.isOptimal).length;

    // Find most common path
    const pathDescriptions: Record<string, number> = {};
    paths.forEach(p => {
      pathDescriptions[p.description] = (pathDescriptions[p.description] || 0) + 1;
    });
    const sorted = Object.entries(pathDescriptions).sort((a, b) => b[1] - a[1]);
    if (sorted.length > 0 && sorted[0]) {
      analysis.mostCommonPath = sorted[0][0];
    }

    // Estimate complexity (0-1)
    analysis.complexityScore = Math.min(1, analysis.averagePathLength / 20);

    return analysis;
  }

  /**
   * Clear extended iteration history for an agent loop
   * @param agentLoopId Agent Loop ID
   */
  async clearHistory(agentLoopId: ID): Promise<void> {
    const toDelete: string[] = [];
    this.iterationDetails.forEach((record, id) => {
      if (record.agentLoopId === agentLoopId) {
        toDelete.push(id);
      }
    });

    toDelete.forEach(id => this.iterationDetails.delete(id));
    logger.debug("Cleared extended iteration history for agent loop", { agentLoopId });
  }
}
