/**
 * Workflow Error Analysis API
 *
 * Workflow-specific implementation for analyzing error chains and root causes.
 *
 * This API provides comprehensive error analysis capabilities:
 * - Error chain tracking with node-level context
 * - Root cause identification with node references
 * - Recovery recommendations tailored for workflow nodes
 * - Node-execution specific error pattern analysis
 *
 * Integrates with WorkflowExecutionState to access error records.
 */

import { QueryableResourceAPI } from "../../../shared/resources/generic-resource-api.js";
import type { APIDependencyManager } from "@sdk/api/shared/core/sdk-dependencies.js";
import type { ID } from "@wf-agent/types";
import type { ExecutionErrorRecord } from "@wf-agent/types";
import type { IErrorAnalysisProvider } from "../../../shared/resources/errors/error-analysis-provider.js";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";
import type { BaseEvent } from "@wf-agent/types";
import { WorkflowExecutionStateAPI } from "../workflow-execution-state-api.js";

const logger = createContextualLogger({ operation: "WorkflowErrorAnalysisAPI" });

// ============================================================================
// Workflow-specific Type Extensions
// ============================================================================

/**
 * Workflow Error Context - extends standard error context
 */
export interface WorkflowErrorContext {
  /** Node ID where error occurred */
  nodeId?: string;
  /** Node name for readability */
  nodeName?: string;
  /** Iteration/Loop context if within loop */
  loopContext?: {
    loopId: string;
    loopName?: string;
    iteration: number;
  };
  /** Sub-workflow context if within subgraph */
  subgraphContext?: {
    parentNodeId: string;
    subgraphWorkflowId: string;
  };
  /** Tool execution details if applicable */
  toolName?: string;
  toolVersion?: string;
  /** LLM node details if applicable */
  llmNodeDetails?: {
    model: string;
    tokensUsed?: number;
  };
  /** Variable snapshot at the time of error */
  variableSnapshot?: Record<string, unknown>;
  /** Call stack at the time of error */
  callStack?: Array<{ id: string; name?: string }>;
  /** Memory usage at the time of error (bytes) */
  memoryUsage?: number;
}

/**
 * Root cause analysis with workflow context
 */
export interface WorkflowRootCauseAnalysis {
  hasError: boolean;
  rootCauseError?: ExecutionErrorRecord;
  errorChain: ExecutionErrorRecord[];
  canRecover: boolean;
  recommendedAction?: "retry" | "fallback" | "skip" | "abort";
  affectedNodes: string[];
  summary: string;
}

/**
 * Error statistics with workflow metrics
 */
export interface WorkflowErrorStatistics {
  totalErrors: number;
  byType: Record<string, number>;
  byNodeId: Record<string, number>;
  byNodeName: Record<string, number>;
  byIteration?: Record<number, number>;
  mostErrorProneNode?: { id: string; name?: string; count: number };
  errorRecoveryRate: number;
  bySeverity: Record<string, number>;
  nodeImpactMap: Map<string, number>;
}

/**
 * Recovery proposal tailored for workflow
 */
export interface WorkflowRecoveryProposal {
  errorId: string;
  action: "retry" | "fallback" | "skip" | "abort";
  affectedNode?: { id: string; name?: string };
  reason: string;
  likelihood: number;
  steps: string[];
  estimatedTimeToRecover?: number;
}

/**
 * Error hotspot - identifies iterations with high error concentration
 */
export interface WorkflowErrorHotspot {
  /** Node ID where errors are concentrated */
  nodeId: string;
  /** Node name for readability */
  nodeName?: string;
  /** Error count */
  errorCount: number;
  /** Error types observed */
  errorTypes: string[];
  /** Highest severity observed */
  severity: string;
}

/**
 * Temporal pattern of error occurrence
 */
export type WorkflowTemporalPattern = 'none' | 'steady' | 'accelerating' | 'decelerating';

/**
 * Error trend direction
 */
export type WorkflowErrorTrend = 'increasing' | 'decreasing' | 'stable';

/**
 * Problematic node in workflow execution
 */
export interface ProblematicNode {
  /** Node ID */
  nodeId: string;
  /** Node name */
  nodeName?: string;
  /** Error count */
  errorCount: number;
  /** Node type */
  nodeType?: string;
}

/**
 * Advanced error analysis with frequency, patterns, and hotspots
 */
export interface AdvancedWorkflowErrorAnalysis {
  /** Total errors */
  totalErrors: number;
  /** Error frequency by type */
  errorFrequency: Record<string, number>;
  /** Error hotspots (nodes with highest error rates) */
  errorHotspots: WorkflowErrorHotspot[];
  /** Temporal pattern of error occurrence */
  temporalPattern: WorkflowTemporalPattern;
  /** Most problematic nodes */
  mostProblematicNodes: ProblematicNode[];
  /** Error trend direction */
  errorTrend: WorkflowErrorTrend;
}

// ============================================================================
// API Implementation
// ============================================================================

/**
 * Workflow Error Analysis API
 *
 * Provides error chain analysis for Workflow executions with workflow-specific insights:
 * - Node-level error tracking
 * - Loop and subgraph context
 * - Workflow topology-aware recovery suggestions
 */
export class WorkflowErrorAnalysisAPI
  extends QueryableResourceAPI<
    ExecutionErrorRecord,
    string,
    { executionId?: ID; nodeId?: string; errorType?: string }
  >
  implements IErrorAnalysisProvider
{
  private deps: APIDependencyManager;

  constructor(deps: APIDependencyManager) {
    super();
    this.deps = deps;
  }

  protected async getResource(id: string): Promise<ExecutionErrorRecord | null> {
    logger.debug("Fetching error record", { errorId: id });
    return null; // Callers should use getErrorChain or getErrorStatistics
  }

  protected async getAllResources(): Promise<ExecutionErrorRecord[]> {
    return [];
  }

  /**
   * Get the complete error chain for a specific error
   *
   * Returns all errors from the root cause up to and including the specified error.
   * If no errorId is provided, returns the chain for the last error.
   */
  async getErrorChain(executionId: ID, fromErrorId?: string): Promise<ExecutionErrorRecord[]> {
    logger.debug("Getting error chain", { executionId, fromErrorId });

    const errorRecords = await this.getExecutionErrorRecords(executionId);
    if (errorRecords.length === 0) {
      return [];
    }

    const targetErrorId = fromErrorId || errorRecords[errorRecords.length - 1]!.id;
    const targetError = errorRecords.find(e => e.id === targetErrorId);

    if (!targetError) {
      logger.warn("Error not found", { executionId, errorId: targetErrorId });
      return [];
    }

    if (!targetError.errorChain) {
      return [targetError];
    }

    return targetError.errorChain
      .map(id => errorRecords.find(e => e.id === id))
      .filter((e): e is ExecutionErrorRecord => Boolean(e));
  }

  /**
   * Analyze root cause of workflow execution failure
   *
   * Identifies the root cause error in the chain and provides analysis.
   */
  async analyzeRootCause(executionId: ID): Promise<WorkflowRootCauseAnalysis> {
    logger.debug("Analyzing root cause", { executionId });

    const errorRecords = await this.getExecutionErrorRecords(executionId);

    if (errorRecords.length === 0) {
      return {
        hasError: false,
        errorChain: [],
        canRecover: false,
        affectedNodes: [],
        summary: "No errors occurred during execution",
      };
    }

    const lastError = errorRecords[errorRecords.length - 1]!;
    const rootError = errorRecords.find(e => e.id === lastError.rootCauseId);

    const errorChain = lastError.errorChain
      ? lastError.errorChain
          .map(id => errorRecords.find(e => e.id === id))
          .filter((e): e is ExecutionErrorRecord => Boolean(e))
      : [lastError];

    const canRecover = errorChain.every(e => e.isRecoverable);
    const recommendedAction = this.getRecommendedWorkflowAction(errorChain);
    const affectedNodes = this.extractAffectedNodes(errorChain);
    const summary = this.generateWorkflowSummary(rootError || lastError, errorChain, affectedNodes);

    return {
      hasError: true,
      rootCauseError: rootError || lastError,
      errorChain,
      canRecover,
      recommendedAction,
      affectedNodes,
      summary,
    };
  }

  /**
   * Get error statistics for an execution
   *
   * Provides aggregated statistics about errors with workflow-specific breakdown:
   * - Total count
   * - Distribution by type, node, iteration, severity
   * - Recovery rate
   * - Node impact analysis
   */
  async getErrorStatistics(executionId: ID): Promise<WorkflowErrorStatistics> {
    logger.debug("Computing error statistics", { executionId });

    const errorRecords = await this.getExecutionErrorRecords(executionId);

    if (errorRecords.length === 0) {
      return {
        totalErrors: 0,
        byType: {},
        byNodeId: {},
        byNodeName: {},
        errorRecoveryRate: 100,
        bySeverity: {},
        nodeImpactMap: new Map(),
      };
    }

    const byType: Record<string, number> = {};
    const byNodeId: Record<string, number> = {};
    const byNodeName: Record<string, number> = {};
    const byIteration: Record<number, number> = {};
    const bySeverity: Record<string, number> = {};
    const nodeImpactMap = new Map<string, number>();

    let recoverableCount = 0;

    errorRecords.forEach(err => {
      byType[err.errorType] = (byType[err.errorType] ?? 0) + 1;

      if (err.iteration !== undefined) {
        byIteration[err.iteration] = (byIteration[err.iteration] ?? 0) + 1;
      }

      bySeverity[err.severity] = (bySeverity[err.severity] ?? 0) + 1;

      if (err.isRecoverable) {
        recoverableCount++;
      }

      // Workflow-specific: track by node
      const nodeId = (err.context as any).nodeId;
      const nodeName = (err.context as any).nodeName;

      if (nodeId) {
        byNodeId[nodeId] = (byNodeId[nodeId] ?? 0) + 1;
        nodeImpactMap.set(nodeId, (nodeImpactMap.get(nodeId) ?? 0) + 1);
      }

      if (nodeName) {
        byNodeName[nodeName] = (byNodeName[nodeName] ?? 0) + 1;
      }
    });

    const mostErrorProneNode = Array.from(nodeImpactMap.entries())
      .sort(([, a], [, b]) => b - a)[0];

    return {
      totalErrors: errorRecords.length,
      byType,
      byNodeId,
      byNodeName,
      byIteration: Object.keys(byIteration).length > 0 ? byIteration : undefined,
      mostErrorProneNode: mostErrorProneNode
        ? { id: mostErrorProneNode[0], count: mostErrorProneNode[1] }
        : undefined,
      errorRecoveryRate: (recoverableCount / errorRecords.length) * 100,
      bySeverity,
      nodeImpactMap,
    };
  }

  /**
   * Get recovery proposal for a specific error
   *
   * Analyzes an error and proposes workflow-specific recovery strategy.
   */
  async getRecoveryProposal(
    executionId: ID,
    errorId: string
  ): Promise<WorkflowRecoveryProposal | null> {
    logger.debug("Getting recovery proposal", { executionId, errorId });

    const errorRecords = await this.getExecutionErrorRecords(executionId);
    const error = errorRecords.find(e => e.id === errorId);

    if (!error) {
      logger.warn("Error not found for recovery proposal", { executionId, errorId });
      return null;
    }

    const action = error.recoveryAction ?? this.suggestWorkflowRecoveryAction(error);
    const likelihood = this.estimateRecoveryLikelihood(error, action);
    const steps = this.generateWorkflowRecoverySteps(error, action);
    const affectedNode = this.extractNodeContext(error);

    return {
      errorId: error.id,
      action,
      affectedNode,
      reason: this.generateRecoveryReason(error, action),
      likelihood,
      steps,
      estimatedTimeToRecover: this.estimateRecoveryTime(error, action),
    };
  }

  /**
   * Get similar errors from error chain
   *
   * Finds other errors in the chain that are similar to the given error.
   */
  async getSimilarErrors(executionId: ID, errorId: string): Promise<ExecutionErrorRecord[]> {
    const errorRecords = await this.getExecutionErrorRecords(executionId);
    const targetError = errorRecords.find(e => e.id === errorId);

    if (!targetError) {
      return [];
    }

    return errorRecords.filter(
      e => e.errorType === targetError.errorType && e.id !== errorId
    );
  }

  /**
   * Get advanced error analysis with frequency, patterns, and hotspots.
   *
   * Provides additional insights:
   * - Error frequency by type over time
   * - Error hotspots (nodes with highest error rates)
   * - Temporal patterns (errors increase/decrease)
   * - Most problematic nodes
   * - Error trend direction
   *
   * @param executionId Execution ID
   * @returns Advanced error analysis
   */
  async getAdvancedErrorAnalysis(executionId: ID): Promise<AdvancedWorkflowErrorAnalysis> {
    logger.debug("Computing advanced error analysis", { executionId });

    const errorRecords = await this.getExecutionErrorRecords(executionId);

    if (errorRecords.length === 0) {
      return {
        totalErrors: 0,
        errorFrequency: {},
        errorHotspots: [],
        temporalPattern: 'none',
        mostProblematicNodes: [],
        errorTrend: 'stable',
      };
    }

    // Analyze error frequency over time
    const sortedErrors = [...errorRecords].sort((a, b) => a.timestamp - b.timestamp);
    const errorFrequency: Record<string, number> = {};
    const nodeProblems: Record<string, { count: number; types: Set<string>; names: string[]; severity: string }> = {};

    sortedErrors.forEach(err => {
      // Frequency by type
      errorFrequency[err.errorType] = (errorFrequency[err.errorType] ?? 0) + 1;

      // Node problems
      const nodeId = (err.context as any).nodeId;
      const nodeName = (err.context as any).nodeName;
      if (nodeId) {
        if (!nodeProblems[nodeId]) {
          nodeProblems[nodeId] = { count: 0, types: new Set(), names: [], severity: err.severity };
        }
        nodeProblems[nodeId]!.count++;
        nodeProblems[nodeId]!.types.add(err.errorType);
        if (nodeName && !nodeProblems[nodeId]!.names.includes(nodeName)) {
          nodeProblems[nodeId]!.names.push(nodeName);
        }
        // Track highest severity
        const severityOrder = ['warning', 'error', 'critical'];
        if (severityOrder.indexOf(err.severity) > severityOrder.indexOf(nodeProblems[nodeId]!.severity)) {
          nodeProblems[nodeId]!.severity = err.severity;
        }
      }
    });

    // Build error hotspots
    const errorHotspots: WorkflowErrorHotspot[] = Object.entries(nodeProblems)
      .sort(([, a], [, b]) => b.count - a.count)
      .map(([nodeId, data]) => ({
        nodeId,
        nodeName: data.names[0],
        errorCount: data.count,
        errorTypes: Array.from(data.types),
        severity: data.severity,
      }));

    // Build most problematic nodes
    const mostProblematicNodes: ProblematicNode[] = errorHotspots.map(hotspot => ({
      nodeId: hotspot.nodeId,
      nodeName: hotspot.nodeName,
      errorCount: hotspot.errorCount,
      nodeType: undefined,
    }));

    // Analyze temporal pattern
    const temporalPattern = this.analyzeTemporalPattern(sortedErrors);
    const errorTrend = this.analyzeErrorTrend(sortedErrors);

    return {
      totalErrors: errorRecords.length,
      errorFrequency,
      errorHotspots: errorHotspots.slice(0, 10),
      temporalPattern,
      mostProblematicNodes: mostProblematicNodes.slice(0, 5),
      errorTrend,
    };
  }

  /**
   * Get error context with execution state snapshot
   *
   * Retrieves the execution context (variables, call stack, memory usage)
   * at the time of a specific error. Integrates with WorkflowExecutionStateAPI
   * to provide enriched error context.
   *
   * @param executionId Execution ID
   * @param errorId Error ID to get context for
   * @returns Workflow error context with execution details, or null if not found
   */
  async getErrorContext(
    executionId: ID,
    errorId: string,
  ): Promise<{ error: ExecutionErrorRecord; context: WorkflowErrorContext } | null> {
    const errorRecords = await this.getExecutionErrorRecords(executionId);
    const error = errorRecords.find(e => e.id === errorId);

    if (!error) {
      logger.warn("Error not found for context retrieval", { executionId, errorId });
      return null;
    }

    // Try to get execution context snapshot
    let variableSnapshot: Record<string, unknown> = {};
    let callStack: Array<{ id: string; name?: string }> = [];
    let memoryUsage: number | undefined;

    try {
      const contextApi = new WorkflowExecutionStateAPI(this.deps);
      const executionContext = await contextApi.getExecutionContext(executionId);
      if (executionContext) {
        callStack = (executionContext.callStack ?? []).map((frame: any) => ({
          id: frame.nodeId,
          name: frame.nodeName,
        }));
        memoryUsage = executionContext.memoryUsage;

        // Get variable snapshot near the error timestamp
        const snapshots = await contextApi.getVariableSnapshotsByTimeRange(executionId, {
          start: error.timestamp - 1000,
          end: error.timestamp + 1000,
        });
        if (snapshots.length > 0) {
          const closest = snapshots.reduce((prev: any, curr: any) =>
            Math.abs(curr.timestamp - error.timestamp) < Math.abs(prev.timestamp - error.timestamp)
              ? curr
              : prev,
          );
          variableSnapshot = Object.fromEntries(
            closest.variables.map((v: any) => [v.name, v.value]),
          );
        }
      }
    } catch (err) {
      logger.debug("Failed to retrieve execution context", { executionId, error: err });
    }

    // Build enriched error context
    const errorContext = error.context as WorkflowErrorContext | undefined;
    const enrichedContext: WorkflowErrorContext = {
      ...errorContext,
      nodeId: errorContext?.nodeId || (error.context as any).nodeId,
      nodeName: errorContext?.nodeName || (error.context as any).nodeName,
      toolName: errorContext?.toolName || (error.context as any).toolName,
      variableSnapshot: Object.keys(variableSnapshot).length > 0 ? variableSnapshot : undefined,
      callStack: callStack.length > 0 ? callStack : undefined,
      memoryUsage,
    };

    return { error, context: enrichedContext };
  }

  /**
   * Get error context chain for an execution
   *
   * Retrieves enriched error contexts for all errors in the chain.
   * Useful for debugging complex error scenarios.
   *
   * @param executionId Execution ID
   * @returns Array of enriched error contexts
   */
  async getErrorContextChain(executionId: ID): Promise<Array<{
    error: ExecutionErrorRecord;
    context: WorkflowErrorContext;
  }>> {
    const errorRecords = await this.getExecutionErrorRecords(executionId);
    const contexts: Array<{ error: ExecutionErrorRecord; context: WorkflowErrorContext }> = [];

    for (const error of errorRecords) {
      const result = await this.getErrorContext(executionId, error.id);
      if (result) {
        contexts.push(result);
      }
    }

    return contexts;
  }

  // ============ Helper Methods ============

  private getRecommendedWorkflowAction(
    errorChain: ExecutionErrorRecord[]
  ): "retry" | "fallback" | "skip" | "abort" {
    if (errorChain.length === 0) return "abort";

    // Check for warnings first (highest priority)
    if (errorChain.some(e => e.severity === "warning")) return "skip";

    const retryCount = errorChain.filter(e => e.recoveryAction === "retry").length;
    const fallbackCount = errorChain.filter(e => e.recoveryAction === "fallback").length;

    if (retryCount >= fallbackCount && errorChain.every(e => e.isRecoverable)) {
      return "retry";
    }
    if (fallbackCount > 0) return "fallback";

    return "abort";
  }

  private suggestWorkflowRecoveryAction(
    error: ExecutionErrorRecord
  ): "retry" | "fallback" | "skip" | "abort" {
    if (error.recoveryAction) {
      return error.recoveryAction;
    }

    switch (error.errorType) {
      case "tool_error":
        return error.isRecoverable ? "retry" : "fallback";
      case "timeout":
        return "retry";
      case "validation_error":
        return error.severity === "warning" ? "skip" : "fallback";
      case "execution_error":
        return error.isRecoverable ? "retry" : "abort";
      default:
        return error.isRecoverable ? "retry" : "abort";
    }
  }

  private estimateRecoveryLikelihood(error: ExecutionErrorRecord, action: string): number {
    let likelihood = 50;

    if (error.isRecoverable) likelihood += 30;
    if (error.context.operation === "tool_call") likelihood += 10;
    if (error.severity === "warning") likelihood += 20;
    if (error.severity === "error") likelihood -= 10;

    if (action === "retry") likelihood += 15;
    if (action === "fallback") likelihood += 10;
    if (action === "skip") likelihood += 20;

    return Math.min(Math.max(likelihood, 0), 100);
  }

  private generateWorkflowRecoverySteps(error: ExecutionErrorRecord, action: string): string[] {
    const steps: string[] = [];
    const nodeInfo = (error.context as any).nodeId ? ` in node ${(error.context as any).nodeId}` : '';

    switch (action) {
      case "retry":
        steps.push(`Wait a moment${nodeInfo}`);
        steps.push("Retry the failed operation");
        steps.push("If still failing, escalate to manual intervention");
        break;

      case "fallback":
        steps.push(`Check if fallback implementation is available${nodeInfo}`);
        steps.push("Switch to fallback implementation");
        steps.push("Continue execution with fallback");
        break;

      case "skip":
        steps.push(`Mark operation${nodeInfo} as skipped`);
        steps.push("Continue with next operation");
        break;

      case "abort":
        steps.push("Log detailed error information");
        steps.push("Cleanup active resources");
        steps.push("Gracefully shutdown workflow execution");
        break;
    }

    return steps;
  }

  private extractAffectedNodes(errorChain: ExecutionErrorRecord[]): string[] {
    const nodeSet = new Set<string>();
    errorChain.forEach(err => {
      const nodeId = (err.context as any).nodeId;
      if (nodeId) nodeSet.add(nodeId);
    });
    return Array.from(nodeSet);
  }

  private extractNodeContext(error: ExecutionErrorRecord): { id: string; name?: string } | undefined {
    const ctx = error.context as any;
    if (ctx.nodeId) {
      return { id: ctx.nodeId, name: ctx.nodeName };
    }
    return undefined;
  }

  private estimateRecoveryTime(_error: ExecutionErrorRecord, action: string): number {
    let timeMs = 100; // base time

    if (action === "retry") timeMs += 1000;
    if (action === "fallback") timeMs += 500;
    if (action === "skip") timeMs += 200;
    if (action === "abort") timeMs = 0; // no recovery

    return timeMs;
  }

  private generateWorkflowSummary(
    rootError: ExecutionErrorRecord,
    chain: ExecutionErrorRecord[],
    affectedNodes: string[]
  ): string {
    if (chain.length === 1) {
      return `Error${affectedNodes.length > 0 ? ` in ${affectedNodes[0]}` : ''}: ${rootError.message}`;
    }

    const chainStr = chain.map((e, i) => `${i + 1}. ${e.errorType}: ${e.message}`).join(" → ");
    const nodesStr = affectedNodes.length > 0 ? `\n\nAffected nodes: ${affectedNodes.join(", ")}` : '';
    return `Root cause: ${rootError.message}\n\nError chain:\n${chainStr}${nodesStr}`;
  }

  private generateRecoveryReason(error: ExecutionErrorRecord, action: string): string {
    if (error.causedBy?.reason) {
      return error.causedBy.reason;
    }

    const nodeInfo = (error.context as any).nodeId ? ` in ${(error.context as any).nodeId}` : '';
    return `${error.errorType}${nodeInfo}: ${action} is recommended based on error properties`;
  }

  /**
   * Implementation of IErrorAnalysisProvider interface
   *
   * Fetches error records from WorkflowExecutionState via WorkflowExecutionRegistry
   */
  async getExecutionErrorRecords(executionId: ID): Promise<ExecutionErrorRecord[]> {
    try {
      const registry = this.deps.getWorkflowExecutionRegistry();
      const entity = await registry.get(executionId);

      if (!entity) {
        logger.debug("Workflow execution not found in registry", { executionId });
        return [];
      }

      return entity.state.getErrorRecords();
    } catch (err) {
      logger.error("Failed to fetch workflow error records", { executionId, error: err });
      return [];
    }
  }

  /**
   * Stream error chain as errors are processed
   *
   * Yields errors one by one from the error chain, allowing callers to
   * process them incrementally without waiting for the full chain.
   *
   * @param executionId Execution ID
   * @returns Async generator yielding errors one at a time
   *
   * @example
   * ```typescript
   * for await (const error of api.streamErrorChain(execId)) {
   *   console.log(`Error: ${error.message}`);
   * }
   * ```
   */
  async *streamErrorChain(executionId: ID): AsyncGenerator<ExecutionErrorRecord, void, void> {
    const errorRecords = await this.getExecutionErrorRecords(executionId);
    if (errorRecords.length === 0) {
      return;
    }

    // Yield errors in order, starting from the root cause
    // Root cause: an error whose rootCauseId is its own id or is undefined (no parent)
    const rootError = errorRecords.find(
      e => !e.rootCauseId || e.rootCauseId === e.id || !e.parentErrorId,
    );
    if (rootError) {
      yield rootError;
      // Yield errors that chain from the root
      for (const error of errorRecords) {
        if (error.id !== rootError.id && error.errorChain?.includes(rootError.id)) {
          yield error;
        }
      }
    }

    // Yield any remaining errors not in the root chain
    const yieldedIds = new Set<string>();
    for (const error of errorRecords) {
      if (!yieldedIds.has(error.id)) {
        yieldedIds.add(error.id);
        yield error;
      }
    }
  }

  /**
   * Subscribe to real-time error events for an execution
   *
   * Uses the event infrastructure to listen for error events
   * and invoke the callback whenever a new error is detected.
   *
   * @param executionId Execution ID to subscribe to
   * @param callback Function to call with each new error record
   * @returns Unsubscribe function to stop listening
   *
   * @example
   * ```typescript
   * const unsubscribe = api.subscribeToErrors(execId, (error) => {
   *   console.log(`New error: ${error.message}`);
   * });
   * // Later: unsubscribe();
   * ```
   */
  subscribeToErrors(
    executionId: ID,
    callback: (error: ExecutionErrorRecord) => void,
  ): () => void {
    const eventManager = this.deps.getEventManager();

    const unsubscribe = eventManager.onGlobal((event: BaseEvent) => {
      // Filter for error events related to this execution
      if (event.type === "ERROR") {
        if (event.executionId === executionId && event.metadata && event.metadata['errorRecord']) {
          const errorRecord = event.metadata['errorRecord'] as ExecutionErrorRecord;
          try {
            callback(errorRecord);
          } catch (cbError) {
            logger.error("Error in subscription callback", {
              executionId,
              error: cbError,
            });
          }
        }
      }
    });

    logger.debug("Subscribed to workflow error events", { executionId });

    return () => {
      unsubscribe();
      logger.debug("Unsubscribed from workflow error events", { executionId });
    };
  }

  /**
   * Analyze temporal pattern of error occurrence
   * Determines if errors are accelerating, decelerating, or steady over time.
   */
  private analyzeTemporalPattern(sortedErrors: ExecutionErrorRecord[]): WorkflowTemporalPattern {
    if (sortedErrors.length < 2) {
      return 'none';
    }

    const timeIntervals: number[] = [];
    for (let i = 1; i < sortedErrors.length; i++) {
      timeIntervals.push(sortedErrors[i]!.timestamp - sortedErrors[i - 1]!.timestamp);
    }

    // If there are not enough intervals to determine pattern, return none
    if (timeIntervals.length < 2) {
      return 'none';
    }

    // If errors are getting closer together, pattern is 'accelerating'
    const recent = timeIntervals.slice(-3);
    const early = timeIntervals.slice(0, 3);

    const recentAvg = recent.reduce((a, b) => a + b, 0) / recent.length;
    const earlyAvg = early.reduce((a, b) => a + b, 0) / early.length;

    if (recentAvg < earlyAvg * 0.7) {
      return 'accelerating';
    } else if (recentAvg > earlyAvg * 1.3) {
      return 'decelerating';
    }

    return 'steady';
  }

  /**
   * Analyze error trend (are errors increasing or decreasing?)
   * Compares the first half of errors with the second half.
   */
  private analyzeErrorTrend(sortedErrors: ExecutionErrorRecord[]): WorkflowErrorTrend {
    if (sortedErrors.length < 2) {
      return 'stable';
    }

    const mid = Math.floor(sortedErrors.length / 2);
    const firstHalf = sortedErrors.slice(0, mid);
    const secondHalf = sortedErrors.slice(mid);

    const ratio = secondHalf.length / firstHalf.length;

    if (ratio > 1.3) {
      return 'increasing';
    } else if (ratio < 0.7) {
      return 'decreasing';
    }

    return 'stable';
  }
}
