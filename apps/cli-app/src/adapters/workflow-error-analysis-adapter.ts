/**
 * Workflow Error Analysis Adapter
 * Encapsulates Workflow Error Analysis API operations for CLI use
 */

import { BaseAdapter } from "./base-adapter.js";
import {
  WorkflowErrorAnalysisAPI,
  type WorkflowRootCauseAnalysis,
  type WorkflowErrorStatistics,
  type WorkflowRecoveryProposal,
} from "@wf-agent/sdk/api";

/**
 * Workflow Error Analysis Adapter
 * Provides CLI-friendly access to workflow execution error diagnostics
 */
export class WorkflowErrorAnalysisAdapter extends BaseAdapter {
  private api: WorkflowErrorAnalysisAPI;

  constructor() {
    super();
    this.api = this.sdk.errorAnalysis;
  }

  /**
   * Get all error records for a workflow execution
   */
  async getExecutionErrorRecords(executionId: string) {
    return this.executeWithErrorHandling(async () => {
      return this.api.getExecutionErrorRecords(executionId);
    }, `Get error records for execution "${executionId}"`);
  }

  /**
   * Analyze root cause of errors in a workflow execution
   */
  async analyzeRootCause(executionId: string): Promise<WorkflowRootCauseAnalysis> {
    return this.executeWithErrorHandling(async () => {
      return this.api.analyzeRootCause(executionId);
    }, `Analyze root cause for execution "${executionId}"`);
  }

  /**
   * Get error statistics for a workflow execution
   */
  async getErrorStatistics(executionId: string): Promise<WorkflowErrorStatistics> {
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
  ): Promise<WorkflowRecoveryProposal | null> {
    return this.executeWithErrorHandling(async () => {
      return this.api.getRecoveryProposal(executionId, errorId);
    }, `Get recovery proposal for error "${errorId}"`);
  }

  /**
   * Get error chain for a workflow execution
   */
  async getErrorChain(executionId: string, fromErrorId?: string) {
    return this.executeWithErrorHandling(async () => {
      return this.api.getErrorChain(executionId, fromErrorId);
    }, `Get error chain for execution "${executionId}"`);
  }
}
