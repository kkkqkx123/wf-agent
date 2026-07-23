/**
 * Agent Error Analysis Adapter
 * Encapsulates Agent Error Analysis API operations for CLI use
 */

import { BaseAdapter } from "./base-adapter.js";
import {
  AgentErrorAnalysisAPI,
  type RootCauseAnalysis,
  type ErrorStatistics,
  type ErrorRecoveryProposal,
} from "@wf-agent/sdk/api";

/**
 * Agent Error Analysis Adapter
 * Provides CLI-friendly access to AgentLoop error diagnostics
 */
export class AgentErrorAnalysisAdapter extends BaseAdapter {
  private api: AgentErrorAnalysisAPI;

  constructor() {
    super();
    this.api = this.sdk.agentErrorAnalysis;
  }

  /**
   * Get all error records for an agent loop execution
   */
  async getExecutionErrorRecords(executionId: string) {
    return this.executeWithErrorHandling(async () => {
      return this.api.getExecutionErrorRecords(executionId);
    }, `Get error records for execution "${executionId}"`);
  }

  /**
   * Analyze root cause of errors in an agent loop execution
   */
  async analyzeRootCause(executionId: string): Promise<RootCauseAnalysis> {
    return this.executeWithErrorHandling(async () => {
      return this.api.analyzeRootCause(executionId);
    }, `Analyze root cause for execution "${executionId}"`);
  }

  /**
   * Get error statistics for an agent loop execution
   */
  async getErrorStatistics(executionId: string): Promise<ErrorStatistics> {
    return this.executeWithErrorHandling(async () => {
      return this.api.getErrorStatistics(executionId);
    }, `Get error statistics for execution "${executionId}"`);
  }

  /**
   * Get recovery proposal for a specific error
   */
  async getRecoveryProposal(
    executionId: string,
    errorId: string,
  ): Promise<ErrorRecoveryProposal | null> {
    return this.executeWithErrorHandling(async () => {
      return this.api.getRecoveryProposal(executionId, errorId);
    }, `Get recovery proposal for error "${errorId}"`);
  }

  /**
   * Get error chain (causal chain of errors)
   */
  async getErrorChain(executionId: string, fromErrorId?: string) {
    return this.executeWithErrorHandling(async () => {
      return this.api.getErrorChain(executionId, fromErrorId);
    }, `Get error chain for execution "${executionId}"`);
  }
}
