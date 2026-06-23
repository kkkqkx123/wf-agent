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
}
