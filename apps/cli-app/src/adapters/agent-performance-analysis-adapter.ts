/**
 * Agent Performance Analysis Adapter
 * Encapsulates Agent Performance Analysis API operations for CLI use
 */

import { BaseAdapter } from "./base-adapter.js";
import {
  AgentPerformanceAnalysisAPI,
  type ExecutionPerformanceProfile,
  type IterationComparison,
  type ExecutionTimelineEntry,
} from "@wf-agent/sdk/api";

/**
 * Agent Performance Analysis Adapter
 * Provides CLI-friendly access to AgentLoop performance diagnostics
 */
export class AgentPerformanceAnalysisAdapter extends BaseAdapter {
  private api: AgentPerformanceAnalysisAPI;

  constructor() {
    super();
    this.api = this.sdk.agentPerformance;
  }

  /**
   * Analyze performance profile for an agent loop execution
   */
  async analyzePerformance(executionId: string): Promise<ExecutionPerformanceProfile | null> {
    return this.executeWithErrorHandling(async () => {
      return this.api.analyzePerformance(executionId);
    }, `Analyze performance for execution "${executionId}"`);
  }

  /**
   * Get iteration comparison (fastest/slowest, trend) for an execution
   */
  async getIterationComparison(executionId: string): Promise<IterationComparison> {
    return this.executeWithErrorHandling(async () => {
      return this.api.getIterationComparison(executionId);
    }, `Get iteration comparison for execution "${executionId}"`);
  }

  /**
   * Get execution timeline entries for an execution
   */
  async getExecutionTimeline(executionId: string): Promise<ExecutionTimelineEntry[]> {
    return this.executeWithErrorHandling(async () => {
      return this.api.getExecutionTimeline(executionId);
    }, `Get execution timeline for "${executionId}"`);
  }
}
