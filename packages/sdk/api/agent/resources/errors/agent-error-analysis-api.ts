/**
 * Agent Error Analysis API
 *
 * Agent-specific implementation for analyzing error chains and root causes.
 *
 * This API provides comprehensive error analysis capabilities:
 * - Error chain tracking
 * - Root cause identification
 * - Recovery recommendations
 * - Error pattern analysis
 *
 * Integrates with AgentLoopState to access error records stored in checkpoint state.
 */

import { QueryableResourceAPI } from "../../../shared/resources/generic-resource-api.js";
import type { APIDependencyManager } from "@sdk/api/shared/core/sdk-dependencies.js";
import type { ID } from "@wf-agent/types";
import type { ExecutionErrorRecord } from "@wf-agent/types";
import type { IErrorAnalysisProvider } from "../../../shared/resources/errors/error-analysis-provider.js";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";

const logger = createContextualLogger({ operation: "AgentErrorAnalysisAPI" });

// ============================================================================
// Type Definitions
// ============================================================================

/**
 * Root cause analysis result
 */
export interface RootCauseAnalysis {
  /** Whether an error occurred during execution */
  hasError: boolean;

  /** Root cause error (first error in chain) */
  rootCauseError?: ExecutionErrorRecord;

  /** Complete error chain from root to leaf */
  errorChain: ExecutionErrorRecord[];

  /** Whether errors can be recovered from */
  canRecover: boolean;

  /** Recommended recovery action */
  recommendedAction?: "retry" | "fallback" | "manual_intervention" | "abort";

  /** Human-readable summary */
  summary: string;
}

/**
 * Error statistics
 */
export interface ErrorStatistics {
  /** Total error count */
  totalErrors: number;

  /** Errors grouped by type */
  byType: Record<string, number>;

  /** Errors grouped by iteration */
  byIteration: Record<number, number>;

  /** Most common error type */
  mostCommonType?: string;

  /** Percentage of recoverable errors */
  errorRecoveryRate: number;

  /** Errors by severity */
  bySeverity: Record<string, number>;
}

/**
 * Error recovery proposal
 */
export interface ErrorRecoveryProposal {
  /** Error ID to recover from */
  errorId: string;

  /** Proposed recovery action */
  action: "retry" | "fallback" | "skip" | "abort";

  /** Reason for this proposal */
  reason: string;

  /** Success likelihood (0-100) */
  likelihood: number;

  /** Steps to execute the recovery */
  steps: string[];
}

// ============================================================================
// API Implementation
// ============================================================================

/**
 * Agent Error Analysis API
 *
 * Provides error chain analysis and root cause identification for Agent Loop executions.
 *
 * Implementation Details:
 * - Retrieves error records from AgentLoopState via AgentLoopRegistry
 * - Supports error chain reconstruction from parentErrorId/errorChain fields
 * - Provides recovery action suggestions based on error characteristics
 */
export class AgentErrorAnalysisAPI
  extends QueryableResourceAPI<
    ExecutionErrorRecord,
    string,
    { executionId?: ID; errorType?: string; severity?: string }
  >
  implements IErrorAnalysisProvider
{
  private deps: APIDependencyManager;

  constructor(deps: APIDependencyManager) {
    super();
    this.deps = deps;
  }

  /**
   * Get error record by ID
   * Retrieves a specific error from the execution's error records
   */
  protected async getResource(id: string): Promise<ExecutionErrorRecord | null> {
    logger.debug("Fetching error record", { errorId: id });
    // Individual error lookup would require tracking execution context
    // For now, return null - callers should use getErrorChain or getErrorStatistics
    return null;
  }

  /**
   * Get all error records for an execution (not recommended for large executions)
   * Use getErrorStatistics or analyzeRootCause instead for analysis
   */
  protected async getAllResources(): Promise<ExecutionErrorRecord[]> {
    return [];
  }

  /**
   * Get the complete error chain for a specific error
   *
   * Returns all errors from the root cause up to and including the specified error.
   * If no errorId is provided, returns the chain for the last error.
   *
   * @param executionId Execution ID
   * @param fromErrorId Error ID to get chain for (optional, defaults to last error)
   * @returns Complete error chain
   *
   * @example
   * const chain = await api.getErrorChain(execId, errorId);
   * // Returns: [root_error, ..., target_error]
   */
  async getErrorChain(executionId: ID, fromErrorId?: string): Promise<ExecutionErrorRecord[]> {
    logger.debug("Getting error chain", { executionId, fromErrorId });

    // Get execution error records from checkpoint
    const errorRecords = await this.getExecutionErrorRecords(executionId);
    if (errorRecords.length === 0) {
      return [];
    }

    // Find target error
    const targetErrorId = fromErrorId || errorRecords[errorRecords.length - 1]!.id;
    const targetError = errorRecords.find(e => e.id === targetErrorId);

    if (!targetError) {
      logger.warn("Error not found", { executionId, errorId: targetErrorId });
      return [];
    }

    // Build chain from errorChain field
    if (!targetError.errorChain) {
      return [targetError];
    }

    return targetError.errorChain
      .map(id => errorRecords.find(e => e.id === id))
      .filter((e): e is ExecutionErrorRecord => Boolean(e));
  }

  /**
   * Analyze root cause of execution failure
   *
   * Identifies the root cause error in the chain and provides analysis.
   *
   * @param executionId Execution ID
   * @returns Root cause analysis
   *
   * @example
   * const analysis = await api.analyzeRootCause(execId);
   * if (analysis.hasError) {
   *   console.log(`Root cause: ${analysis.rootCauseError?.message}`);
   *   console.log(`Recovery action: ${analysis.recommendedAction}`);
   * }
   */
  async analyzeRootCause(executionId: ID): Promise<RootCauseAnalysis> {
    logger.debug("Analyzing root cause", { executionId });

    const errorRecords = await this.getExecutionErrorRecords(executionId);

    if (errorRecords.length === 0) {
      return {
        hasError: false,
        errorChain: [],
        canRecover: false,
        summary: "No errors occurred during execution",
      };
    }

    const lastError = errorRecords[errorRecords.length - 1]!;

    // Find root cause using rootCauseId
    const rootError = errorRecords.find(e => e.id === lastError.rootCauseId);

    // Build complete error chain
    const errorChain = lastError.errorChain
      ? lastError.errorChain
          .map(id => errorRecords.find(e => e.id === id))
          .filter((e): e is ExecutionErrorRecord => Boolean(e))
      : [lastError];

    // Determine if recoverable
    const canRecover = errorChain.every(e => e.isRecoverable);

    // Get recommended action
    const recommendedAction = this.getRecommendedAction(errorChain);

    // Generate summary
    const summary = this.generateSummary(rootError || lastError, errorChain);

    return {
      hasError: true,
      rootCauseError: rootError || lastError,
      errorChain,
      canRecover,
      recommendedAction,
      summary,
    };
  }

  /**
   * Get error statistics for an execution
   *
   * Provides aggregated statistics about errors:
   * - Total count
   * - Distribution by type, iteration, severity
   * - Recovery rate
   *
   * @param executionId Execution ID
   * @returns Error statistics
   *
   * @example
   * const stats = await api.getErrorStatistics(execId);
   * console.log(`Total errors: ${stats.totalErrors}`);
   * console.log(`Recovery rate: ${stats.errorRecoveryRate.toFixed(1)}%`);
   */
  async getErrorStatistics(executionId: ID): Promise<ErrorStatistics> {
    logger.debug("Computing error statistics", { executionId });

    const errorRecords = await this.getExecutionErrorRecords(executionId);

    if (errorRecords.length === 0) {
      return {
        totalErrors: 0,
        byType: {},
        byIteration: {},
        bySeverity: {},
        errorRecoveryRate: 100,
      };
    }

    // Count by type
    const byType: Record<string, number> = {};
    // Count by iteration
    const byIteration: Record<number, number> = {};
    // Count by severity
    const bySeverity: Record<string, number> = {};

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
    });

    return {
      totalErrors: errorRecords.length,
      byType,
      byIteration,
      bySeverity,
      mostCommonType: Object.entries(byType).sort(([, a], [, b]) => b - a)[0]?.[0],
      errorRecoveryRate: (recoverableCount / errorRecords.length) * 100,
    };
  }

  /**
   * Get advanced error analysis with frequency and patterns (P1 enhancement)
   *
   * Provides additional insights:
   * - Error frequency by type over time
   * - Error hot spots (most problematic tools)
   * - Temporal patterns (errors increase/decrease)
   *
   * @param executionId Execution ID
   * @returns Advanced error analysis
   */
  async getAdvancedErrorAnalysis(executionId: ID): Promise<AdvancedErrorAnalysis> {
    logger.debug("Computing advanced error analysis", { executionId });

    const errorRecords = await this.getExecutionErrorRecords(executionId);

    if (errorRecords.length === 0) {
      return {
        totalErrors: 0,
        errorFrequency: {},
        errorHotspots: [],
        temporalPattern: 'none',
        mostProblematicTools: [],
        errorTrend: 'stable',
      };
    }

    // Analyze error frequency over time
    const sortedErrors = [...errorRecords].sort((a, b) => a.timestamp - b.timestamp);
    const errorFrequency: Record<string, number> = {};
    const toolProblems: Record<string, number> = {};

    sortedErrors.forEach(err => {
      // Frequency by type
      errorFrequency[err.errorType] = (errorFrequency[err.errorType] ?? 0) + 1;

      // Tool problems
      if (err.context.toolName) {
        toolProblems[err.context.toolName] = (toolProblems[err.context.toolName] ?? 0) + 1;
      }
    });

    // Get most problematic tools
    const mostProblematicTools = Object.entries(toolProblems)
      .sort(([, a], [, b]) => b - a)
      .slice(0, 5)
      .map(([name, count]) => ({ name, errorCount: count }));

    // Analyze temporal pattern
    const temporalPattern = this.analyzeTemporalPattern(sortedErrors);

    // Analyze trend
    const errorTrend = this.analyzeErrorTrend(sortedErrors);

    // Find error hotspots (iterations with highest error rates)
    const errorHotspots: ErrorHotspot[] = [];
    const errorsByIteration: Record<number, ExecutionErrorRecord[]> = {};

    sortedErrors.forEach(err => {
      if (err.iteration !== undefined) {
        if (!errorsByIteration[err.iteration]) {
          errorsByIteration[err.iteration] = [];
        }
        errorsByIteration[err.iteration].push(err);
      }
    });

    for (const [iteration, errors] of Object.entries(errorsByIteration)) {
      if (errors.length > 0) {
        errorHotspots.push({
          iteration: parseInt(iteration),
          errorCount: errors.length,
          errorTypes: Array.from(new Set(errors.map(e => e.errorType))),
          severity: errors[0]?.severity || 'error',
        });
      }
    }

    return {
      totalErrors: errorRecords.length,
      errorFrequency,
      errorHotspots: errorHotspots.sort((a, b) => b.errorCount - a.errorCount),
      temporalPattern,
      mostProblematicTools,
      errorTrend,
    };
  }

  /**
   * Get recovery proposal for a specific error
   *
   * Analyzes an error and proposes recovery strategy.
   *
   * @param executionId Execution ID
   * @param errorId Error ID to propose recovery for
   * @returns Recovery proposal
   *
   * @example
   * const proposal = await api.getRecoveryProposal(execId, errorId);
   * console.log(`Recommended action: ${proposal.action}`);
   * console.log(`Success likelihood: ${proposal.likelihood}%`);
   */
  async getRecoveryProposal(executionId: ID, errorId: string): Promise<ErrorRecoveryProposal | null> {
    logger.debug("Getting recovery proposal", { executionId, errorId });

    const errorRecords = await this.getExecutionErrorRecords(executionId);
    const error = errorRecords.find(e => e.id === errorId);

    if (!error) {
      logger.warn("Error not found for recovery proposal", { executionId, errorId });
      return null;
    }

    // Determine recovery action
    const action = error.recoveryAction ?? this.suggestRecoveryAction(error);

    // Estimate likelihood
    const likelihood = this.estimateRecoveryLikelihood(error, action);

    // Generate steps
    const steps = this.generateRecoverySteps(error, action);

    return {
      errorId: error.id,
      action,
      reason: this.generateRecoveryReason(error, action),
      likelihood,
      steps,
    };
  }

  /**
   * Get similar errors from error chain
   *
   * Finds other errors in the chain that are similar to the given error.
   *
   * @param executionId Execution ID
   * @param errorId Error ID to find similar errors for
   * @returns Similar errors
   *
   * @example
   * const similar = await api.getSimilarErrors(execId, errorId);
   * console.log(`Found ${similar.length} similar errors`);
   */
  async getSimilarErrors(executionId: ID, errorId: string): Promise<ExecutionErrorRecord[]> {
    const errorRecords = await this.getExecutionErrorRecords(executionId);
    const targetError = errorRecords.find(e => e.id === errorId);

    if (!targetError) {
      return [];
    }

    // Find errors of the same type
    return errorRecords.filter(
      e => e.errorType === targetError.errorType && e.id !== errorId
    );
  }

  // ============ Helper Methods ============

  private getRecommendedAction(
    errorChain: ExecutionErrorRecord[]
  ): "retry" | "fallback" | "manual_intervention" | "abort" {
    if (errorChain.length === 0) return "abort";

    // Count recovery actions
    const retryCount = errorChain.filter(e => e.recoveryAction === "retry").length;
    const fallbackCount = errorChain.filter(e => e.recoveryAction === "fallback").length;

    if (retryCount >= fallbackCount) return "retry";
    if (fallbackCount > 0) return "fallback";

    // Default based on recoverability
    if (errorChain.every(e => e.isRecoverable)) {
      return "retry";
    }

    return "manual_intervention";
  }

  private generateSummary(rootError: ExecutionErrorRecord, chain: ExecutionErrorRecord[]): string {
    if (chain.length === 1) {
      return `Error: ${rootError.message}`;
    }

    const chainStr = chain.map((e, i) => `${i + 1}. ${e.errorType}: ${e.message}`).join(" → ");
    return `Root cause: ${rootError.message}\n\nError chain:\n${chainStr}`;
  }

  private suggestRecoveryAction(error: ExecutionErrorRecord): "retry" | "fallback" | "skip" | "abort" {
    if (error.recoveryAction) {
      return error.recoveryAction;
    }

    // Heuristics based on error type
    switch (error.errorType) {
      case "tool_error":
        return error.isRecoverable ? "retry" : "fallback";
      case "timeout":
        return "retry";
      case "validation_error":
        return "fallback";
      default:
        return error.isRecoverable ? "retry" : "abort";
    }
  }

  private estimateRecoveryLikelihood(
    error: ExecutionErrorRecord,
    action: string
  ): number {
    let likelihood = 50;

    // Adjust based on error properties
    if (error.isRecoverable) likelihood += 30;
    if (error.context.operation === "tool_call") likelihood += 10;
    if (error.severity === "warning") likelihood += 20;
    if (error.severity === "error") likelihood -= 10;

    // Adjust based on recovery action
    if (action === "retry") likelihood += 15;
    if (action === "fallback") likelihood += 10;

    return Math.min(Math.max(likelihood, 0), 100);
  }

  private generateRecoverySteps(error: ExecutionErrorRecord, action: string): string[] {
    const steps: string[] = [];

    switch (action) {
      case "retry":
        steps.push("Wait a moment");
        steps.push("Retry the failed operation");
        steps.push("If still failing, escalate to manual intervention");
        break;

      case "fallback":
        steps.push(`Check if fallback implementation is available for ${error.context.toolName || 'operation'}`);
        steps.push("Switch to fallback implementation");
        steps.push("Continue execution with fallback");
        break;

      case "skip":
        steps.push("Mark operation as skipped");
        steps.push("Continue with next operation");
        break;

      case "abort":
        steps.push("Log detailed error information");
        steps.push("Gracefully shutdown execution");
        break;
    }

    return steps;
  }

  private generateRecoveryReason(error: ExecutionErrorRecord, action: string): string {
    if (error.causedBy?.reason) {
      return error.causedBy.reason;
    }

    const toolInfo = error.context.toolName ? ` in ${error.context.toolName}` : '';
    return `${error.errorType}${toolInfo}: ${action} is recommended based on error properties`;
  }

  /**
   * Analyze temporal pattern of errors
   */
  private analyzeTemporalPattern(sortedErrors: ExecutionErrorRecord[]): TemporalPattern {
    if (sortedErrors.length < 2) {
      return 'none';
    }

    const timeIntervals: number[] = [];
    for (let i = 1; i < sortedErrors.length; i++) {
      timeIntervals.push(sortedErrors[i]!.timestamp - sortedErrors[i - 1]!.timestamp);
    }

    const avgInterval = timeIntervals.reduce((a, b) => a + b, 0) / timeIntervals.length;

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
   */
  private analyzeErrorTrend(sortedErrors: ExecutionErrorRecord[]): ErrorTrend {
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

  /**
   * Implementation of IErrorAnalysisProvider interface
   *
   * Fetches error records from AgentLoopState via AgentLoopRegistry
   */
  async getExecutionErrorRecords(executionId: ID): Promise<ExecutionErrorRecord[]> {
    try {
      const registry = this.deps.getAgentLoopRegistry();
      const entity = await registry.get(executionId);

      if (!entity) {
        logger.debug("Agent execution not found in registry", { executionId });
        return [];
      }

      return entity.state.getErrorRecords();
    } catch (error) {
      logger.error("Failed to fetch agent error records", { executionId, error });
      return [];
    }
  }
}

// ============================================================================
// New Types for Advanced Error Analysis (P1)
// ============================================================================

/**
 * Error hotspot - iteration with high error concentration
 */
export interface ErrorHotspot {
  /** Iteration number */
  iteration: number;
  /** Number of errors in this iteration */
  errorCount: number;
  /** Types of errors */
  errorTypes: string[];
  /** Most severe error in this iteration */
  severity: string;
}

/**
 * Temporal pattern of errors
 */
export type TemporalPattern = 'none' | 'steady' | 'accelerating' | 'decelerating';

/**
 * Error trend (increasing/decreasing/stable)
 */
export type ErrorTrend = 'increasing' | 'decreasing' | 'stable';

/**
 * Problematic tool entry
 */
export interface ProblematicTool {
  /** Tool name */
  name: string;
  /** Number of errors */
  errorCount: number;
}

/**
 * Advanced error analysis result
 */
export interface AdvancedErrorAnalysis {
  /** Total error count */
  totalErrors: number;
  /** Error frequency by type */
  errorFrequency: Record<string, number>;
  /** Iterations with high error concentration */
  errorHotspots: ErrorHotspot[];
  /** Temporal pattern of errors */
  temporalPattern: TemporalPattern;
  /** Most problematic tools */
  mostProblematicTools: ProblematicTool[];
  /** Trend of errors over execution */
  errorTrend: ErrorTrend;
}
