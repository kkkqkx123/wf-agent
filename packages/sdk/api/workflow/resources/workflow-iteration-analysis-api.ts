/**
 * WorkflowIterationAnalysisAPI - Workflow Node Execution Analysis API
 *
 * Provides fine-grained analysis of workflow node execution with Agent-style detailed tracking.
 * Consolidates:
 * - Extended node execution details with tool dependencies
 * - LLM reasoning steps for LLM nodes
 * - Resource consumption and performance metrics
 * - Execution path optimization analysis
 *
 * Phase 1 Implementation: Enhance Workflow's node-level analysis capabilities
 */

import { QueryableResourceAPI } from "../../shared/resources/generic-resource-api.js";
import type { APIDependencyManager } from "@sdk/api/shared/core/sdk-dependencies.js";
import type { ID } from "@wf-agent/types";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ operation: "WorkflowIterationAnalysisAPI" });

// ============================================================================
// Type Definitions: Tool Dependency Tracking
// ============================================================================

/**
 * Tool Dependency relationship in workflow
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

// ============================================================================
// Type Definitions: Execution Path Analysis
// ============================================================================

/**
 * Execution path step
 */
export interface ExecutionPathStep {
  stepId: string;
  type: "node_execution" | "condition_check" | "branch_decision" | "tool_call";
  description: string;
  result?: unknown;
  timestamp: number;
  duration?: number;
}

/**
 * Execution path record
 */
export interface ExecutionPath {
  /** Unique path identifier */
  pathId: string;
  /** Path description */
  description: string;
  /** Sequence of steps taken */
  steps: ExecutionPathStep[];
  /** Whether this path was optimal */
  isOptimal?: boolean;
  /** Alternative paths considered */
  alternativePaths?: string[];
  /** Path complexity score (0-1) */
  complexityScore?: number;
}

// ============================================================================
// Type Definitions: LLM Node Metadata
// ============================================================================

/**
 * LLM reasoning step
 */
export interface LLMReasoningRecord {
  /** Reasoning step identifier */
  stepId: string;
  /** Type of reasoning */
  reasoningType: "thinking" | "planning" | "analyzing" | "evaluating" | "synthesizing";
  /** Reasoning content */
  content: string;
  /** Confidence in this reasoning (0-1) */
  confidence?: number;
  /** Related entities */
  relatedEntities?: string[];
  /** Conclusions drawn */
  conclusions?: string[];
}

/**
 * LLM node metadata
 */
export interface LLMNodeMetadata {
  /** Prompt used for LLM node */
  prompt: string;
  /** LLM model ID */
  modelId: string;
  /** Temperature and other parameters */
  parameters?: Record<string, unknown>;
  /** LLM reasoning steps */
  reasoningSteps?: LLMReasoningRecord[];
  /** LLM thinking process (raw) */
  thinkingProcess?: string;
}

// ============================================================================
// Type Definitions: Resource Tracking
// ============================================================================

/**
 * Resource usage metrics
 */
export interface ResourceUsageRecord {
  /** LLM tokens used (input) */
  llmInputTokens?: number;
  /** LLM tokens used (output) */
  llmOutputTokens?: number;
  /** Total LLM cost */
  llmCost?: number;
  /** API calls made */
  apiCalls?: number;
  /** Data processed (bytes) */
  dataProcessed?: number;
  /** Memory peak usage (bytes) */
  memoryPeak?: number;
  /** Execution time breakdown */
  timingBreakdown?: {
    setupTime?: number;
    executionTime?: number;
    cleanupTime?: number;
  };
}

// ============================================================================
// Type Definitions: Quality Metrics
// ============================================================================

/**
 * Quality assessment for node execution
 */
export interface QualityMetrics {
  /** Quality score (0-1) */
  executionQuality: number;
  /** Whether output requires revision */
  requiresRevision?: boolean;
  /** Optimization opportunities */
  optimizationOpportunities?: string[];
  /** Tags for categorization */
  tags?: string[];
}

// ============================================================================
// Type Definitions: Extended Node Execution
// ============================================================================

/**
 * Extended Node Execution Record
 * Enhances NodeExecutionRecord with Agent-style detailed analysis
 */
export interface ExtendedNodeExecutionRecord {
  /** Workflow execution ID */
  executionId: ID;
  /** Node ID */
  nodeId: string;
  /** Node name */
  nodeName: string;
  /** Node type */
  nodeType: string;
  /** Execution status */
  status: "pending" | "running" | "completed" | "failed" | "skipped" | "cancelled";
  /** Start timestamp */
  startTime: number;
  /** End timestamp */
  endTime?: number;
  /** Duration (ms) */
  duration?: number;
  /** Node input */
  input?: Record<string, unknown>;
  /** Node output */
  output?: unknown;
  /** Retry count */
  retryCount: number;
  /** Error information */
  error?: {
    message: string;
    code?: string;
    details?: Record<string, unknown>;
  };

  // ============================================================================
  // Enhanced Analysis Fields
  // ============================================================================

  /** Tool execution dependencies */
  toolDependencies?: ToolDependency[];

  /** Execution path taken */
  executionPath?: ExecutionPath;

  /** LLM node metadata (for LLM nodes) */
  llmMetadata?: LLMNodeMetadata;

  /** Resource usage metrics */
  resourceUsage?: ResourceUsageRecord;

  /** Quality metrics */
  qualityMetrics?: QualityMetrics;
}

// ============================================================================
// Query & Filter Types
// ============================================================================

/**
 * Extended node execution filter
 */
export interface ExtendedNodeExecutionFilter {
  /** Workflow execution ID list */
  executionIds?: ID[];
  /** Node ID filter */
  nodeId?: string;
  /** Node type filter */
  nodeType?: string;
  /** Status filter */
  status?: string;
  /** Min quality score */
  minQualityScore?: number;
  /** Filter by errors */
  hasErrors?: boolean;
  /** Filter by tool dependencies */
  hasDependencies?: boolean;
  /** Time range */
  timeRange?: {
    start?: number;
    end?: number;
  };
  /** Min duration threshold (ms) */
  minDuration?: number;
}

/**
 * Node execution statistics
 */
export interface NodeExecutionStats {
  /** Total node executions */
  totalExecutions: number;
  /** Successful executions */
  successCount: number;
  /** Failed executions */
  failureCount: number;
  /** Average quality score */
  averageQualityScore: number;
  /** Nodes requiring revision */
  revisionsRequired: number;
  /** Total tool dependencies */
  totalDependencies: number;
  /** Average execution time (ms) */
  averageExecutionTime: number;
  /** Min execution time (ms) */
  minExecutionTime: number;
  /** Max execution time (ms) */
  maxExecutionTime: number;
  /** Total resource usage */
  totalResourceUsage?: {
    totalLLMTokens?: number;
    totalApiCalls?: number;
    peakMemory?: number;
  };
}

/**
 * Optimization opportunity
 */
export interface OptimizationOpportunity {
  /** Node ID */
  nodeId: string;
  /** Node name */
  nodeName: string;
  /** Opportunity description */
  description: string;
  /** Impact level */
  impactLevel: "low" | "medium" | "high";
  /** Estimated improvement */
  estimatedImprovement?: string;
}

// ============================================================================
// API Implementation
// ============================================================================

/**
 * WorkflowIterationAnalysisAPI - Workflow Node Execution Analysis API
 */
export class WorkflowIterationAnalysisAPI extends QueryableResourceAPI<
  ExtendedNodeExecutionRecord,
  string,
  ExtendedNodeExecutionFilter
> {
  private nodeExecutionDetails: Map<string, ExtendedNodeExecutionRecord> = new Map();

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
   * Get extended node execution by ID
   */
  protected override async getResource(id: string): Promise<ExtendedNodeExecutionRecord | null> {
    return this.nodeExecutionDetails.get(id) ?? null;
  }

  /**
   * Get all extended node executions
   */
  protected override async getAllResources(): Promise<ExtendedNodeExecutionRecord[]> {
    return Array.from(this.nodeExecutionDetails.values());
  }

  /**
   * Apply filters to node executions
   */
  protected override applyFilter(
    records: ExtendedNodeExecutionRecord[],
    filter: ExtendedNodeExecutionFilter,
  ): ExtendedNodeExecutionRecord[] {
    let filtered = records;

    if (filter.executionIds && filter.executionIds.length > 0) {
      const idSet = new Set(filter.executionIds);
      filtered = filtered.filter(r => idSet.has(r.executionId));
    }

    if (filter.nodeId) {
      filtered = filtered.filter(r => r.nodeId === filter.nodeId);
    }

    if (filter.nodeType) {
      filtered = filtered.filter(r => r.nodeType === filter.nodeType);
    }

    if (filter.status) {
      filtered = filtered.filter(r => r.status === filter.status);
    }

    if (filter.minQualityScore !== undefined) {
      filtered = filtered.filter(r => (r.qualityMetrics?.executionQuality ?? 0) >= filter.minQualityScore!);
    }

    if (filter.hasErrors) {
      filtered = filtered.filter(r => r.error !== undefined);
    }

    if (filter.hasDependencies) {
      filtered = filtered.filter(r => r.toolDependencies && r.toolDependencies.length > 0);
    }

    if (filter.minDuration !== undefined) {
      filtered = filtered.filter(r => (r.duration ?? 0) >= filter.minDuration!);
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

  // ============================================================================
  // Node Analysis Methods
  // ============================================================================

  /**
   * Record extended node execution detail
   */
  async recordNodeAnalysis(detail: ExtendedNodeExecutionRecord): Promise<void> {
    const id = `${detail.executionId}:node:${detail.nodeId}`;
    this.nodeExecutionDetails.set(id, detail);
    logger.debug("Recorded extended node execution detail", {
      nodeId: detail.nodeId,
      executionId: detail.executionId,
    });
  }

  /**
   * Get node analysis for a specific node in an execution
   */
  async getNodeAnalysis(executionId: ID, nodeId: string): Promise<ExtendedNodeExecutionRecord | null> {
    const id = `${executionId}:node:${nodeId}`;
    return this.nodeExecutionDetails.get(id) ?? null;
  }

  /**
   * Get all node executions for a workflow execution
   */
  async getExecutionNodeAnalyses(executionId: ID): Promise<ExtendedNodeExecutionRecord[]> {
    return Array.from(this.nodeExecutionDetails.values())
      .filter(r => r.executionId === executionId)
      .sort((a, b) => a.startTime - b.startTime);
  }

  /**
   * Get tool dependency chain for a node
   */
  async getToolDependencyChain(executionId: ID, nodeId: string): Promise<ToolDependency[]> {
    const analysis = await this.getNodeAnalysis(executionId, nodeId);
    return analysis?.toolDependencies ?? [];
  }

  /**
   * Get execution path for a workflow execution
   */
  async getExecutionPath(executionId: ID): Promise<ExecutionPath | null> {
    const executions = await this.getExecutionNodeAnalyses(executionId);

    if (executions.length === 0) {
      return null;
    }

    const steps: ExecutionPathStep[] = executions.map(exec => ({
      stepId: exec.nodeId,
      type: "node_execution",
      description: `Execute ${exec.nodeName} (${exec.nodeType})`,
      result: exec.output,
      timestamp: exec.startTime,
      duration: exec.duration,
    }));

    return {
      pathId: `path-${executionId}`,
      description: `Execution path for workflow ${executionId}`,
      steps,
      isOptimal: this.assessPathOptimality(executions),
    };
  }

  /**
   * Get LLM reasoning for an LLM node
   */
  async getLLMReasoningPath(executionId: ID, nodeId: string): Promise<LLMReasoningRecord[]> {
    const analysis = await this.getNodeAnalysis(executionId, nodeId);
    return analysis?.llmMetadata?.reasoningSteps ?? [];
  }

  /**
   * Get optimization opportunities for an execution
   */
  async getOptimizationOpportunities(executionId: ID): Promise<OptimizationOpportunity[]> {
    const executions = await this.getExecutionNodeAnalyses(executionId);
    const opportunities: OptimizationOpportunity[] = [];

    executions.forEach(exec => {
      // Check for long-running nodes
      if (exec.duration && exec.duration > 5000) {
        opportunities.push({
          nodeId: exec.nodeId,
          nodeName: exec.nodeName,
          description: `Node execution took ${exec.duration}ms - consider optimization`,
          impactLevel: "medium",
          estimatedImprovement: `Reduce duration from ${exec.duration}ms`,
        });
      }

      // Check for failed nodes with retries
      if (exec.retryCount > 2) {
        opportunities.push({
          nodeId: exec.nodeId,
          nodeName: exec.nodeName,
          description: `Node retried ${exec.retryCount} times - review error handling`,
          impactLevel: "high",
          estimatedImprovement: "Improve error handling or node logic",
        });
      }

      // Check for complex tool dependencies
      if (exec.toolDependencies && exec.toolDependencies.length > 5) {
        opportunities.push({
          nodeId: exec.nodeId,
          nodeName: exec.nodeName,
          description: `${exec.toolDependencies.length} tool dependencies - consider simplification`,
          impactLevel: "medium",
          estimatedImprovement: "Reduce tool dependency complexity",
        });
      }

      // Include recorded optimization opportunities
      if (exec.qualityMetrics?.optimizationOpportunities) {
        exec.qualityMetrics.optimizationOpportunities.forEach(opp => {
          opportunities.push({
            nodeId: exec.nodeId,
            nodeName: exec.nodeName,
            description: opp,
            impactLevel: "low",
          });
        });
      }
    });

    return opportunities.sort((a, b) => {
      const impactOrder = { high: 0, medium: 1, low: 2 };
      return impactOrder[a.impactLevel] - impactOrder[b.impactLevel];
    });
  }

  /**
   * Get node execution statistics
   */
  async getNodeExecutionStats(filter?: ExtendedNodeExecutionFilter): Promise<NodeExecutionStats> {
    let executions = Array.from(this.nodeExecutionDetails.values());

    if (filter) {
      executions = this.applyFilter(executions, filter);
    }

    const stats: NodeExecutionStats = {
      totalExecutions: executions.length,
      successCount: 0,
      failureCount: 0,
      averageQualityScore: 0,
      revisionsRequired: 0,
      totalDependencies: 0,
      averageExecutionTime: 0,
      minExecutionTime: Infinity,
      maxExecutionTime: 0,
    };

    let qualityScoreSum = 0;
    let durationSum = 0;
    let dependencySum = 0;
    let totalLLMTokens = 0;
    let totalApiCalls = 0;
    let peakMemory = 0;

    executions.forEach(exec => {
      if (exec.status === "completed") {
        stats.successCount++;
      }
      if (exec.status === "failed") {
        stats.failureCount++;
      }

      if (exec.qualityMetrics) {
        qualityScoreSum += exec.qualityMetrics.executionQuality;
        if (exec.qualityMetrics.requiresRevision) {
          stats.revisionsRequired++;
        }
      }

      if (exec.toolDependencies) {
        stats.totalDependencies += exec.toolDependencies.length;
        dependencySum += exec.toolDependencies.length;
      }

      if (exec.duration) {
        durationSum += exec.duration;
        stats.minExecutionTime = Math.min(stats.minExecutionTime, exec.duration);
        stats.maxExecutionTime = Math.max(stats.maxExecutionTime, exec.duration);
      }

      if (exec.resourceUsage) {
        if (exec.resourceUsage.llmInputTokens) {
          totalLLMTokens += exec.resourceUsage.llmInputTokens;
        }
        if (exec.resourceUsage.llmOutputTokens) {
          totalLLMTokens += exec.resourceUsage.llmOutputTokens;
        }
        if (exec.resourceUsage.apiCalls) {
          totalApiCalls += exec.resourceUsage.apiCalls;
        }
        if (exec.resourceUsage.memoryPeak) {
          peakMemory = Math.max(peakMemory, exec.resourceUsage.memoryPeak);
        }
      }
    });

    if (executions.length > 0) {
      stats.averageQualityScore = qualityScoreSum / executions.length;
      stats.averageExecutionTime = durationSum / executions.length;
    }

    if (stats.minExecutionTime === Infinity) {
      stats.minExecutionTime = 0;
    }

    if (totalLLMTokens > 0 || totalApiCalls > 0 || peakMemory > 0) {
      stats.totalResourceUsage = {
        totalLLMTokens: totalLLMTokens || undefined,
        totalApiCalls: totalApiCalls || undefined,
        peakMemory: peakMemory || undefined,
      };
    }

    return stats;
  }

  /**
   * Query node executions by type
   */
  async getNodeExecutionsByType(executionId: ID, nodeType: string): Promise<ExtendedNodeExecutionRecord[]> {
    return Array.from(this.nodeExecutionDetails.values())
      .filter(r => r.executionId === executionId && r.nodeType === nodeType)
      .sort((a, b) => a.startTime - b.startTime);
  }

  /**
   * Get failed nodes in an execution
   */
  async getFailedNodes(executionId: ID): Promise<ExtendedNodeExecutionRecord[]> {
    return Array.from(this.nodeExecutionDetails.values())
      .filter(r => r.executionId === executionId && r.status === "failed")
      .sort((a, b) => b.startTime - a.startTime);
  }

  /**
   * Clear node analysis data for an execution
   */
  async clearExecutionAnalysis(executionId: ID): Promise<void> {
    const toDelete: string[] = [];
    this.nodeExecutionDetails.forEach((record, id) => {
      if (record.executionId === executionId) {
        toDelete.push(id);
      }
    });

    toDelete.forEach(id => this.nodeExecutionDetails.delete(id));
    logger.debug("Cleared node analysis data for workflow execution", { executionId });
  }

  // ============================================================================
  // Helper Methods
  // ============================================================================

  private assessPathOptimality(executions: ExtendedNodeExecutionRecord[]): boolean {
    if (executions.length === 0) {
      return true;
    }

    // Simple heuristic: path is optimal if no nodes failed and few retries
    const hasFailures = executions.some(e => e.status === "failed");
    const totalRetries = executions.reduce((sum, e) => sum + e.retryCount, 0);

    return !hasFailures && totalRetries <= 1;
  }
}
