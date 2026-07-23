/**
 * Iteration Analysis Adapter
 * Uses AgentLoopIterationAPI from SDK for real iteration tracking
 */

import { BaseAdapter } from "./base-adapter.js";
import type { ID } from "@wf-agent/types";
import type { AgentLoopIterationAPI } from "@wf-agent/sdk/api";

export interface IterationSummary {
  iteration: number;
  startTime?: number;
  endTime?: number;
  status: string;
  toolCalls?: number;
  errors?: number;
}

export interface LoopIterationSummary {
  totalIterations: number;
  currentIteration?: number;
  maxIterations?: number;
  status: string;
}

/**
 * Iteration Analysis Adapter
 * Provides methods for querying and analyzing agent loop iterations
 * using the SDK's AgentLoopIterationAPI for real data retrieval.
 */
export class IterationAnalysisAdapter extends BaseAdapter {
  /**
   * Get the AgentLoopIterationAPI instance from SDK
   */
  private getAPI(): AgentLoopIterationAPI {
    return this.sdk.agentLoopIteration;
  }

  /**
   * Get iteration history summary for an agent loop
   */
  async getIterationHistorySummary(
    agentLoopId: ID,
  ): Promise<LoopIterationSummary | null> {
    return this.executeWithErrorHandling(async () => {
      const api = this.getAPI();
      const summary = await api.getExtendedHistorySummary(agentLoopId);

      if (!summary) return null;

      return {
        totalIterations: summary.totalIterations,
        currentIteration: summary.totalIterations,
        maxIterations: summary.totalIterations,
        status: summary.status,
      };
    }, "getIterationHistorySummary");
  }

  /**
   * List all iterations for an agent loop
   */
  async listIterations(
    agentLoopId: ID,
    limit?: number,
  ): Promise<IterationSummary[]> {
    return this.executeWithErrorHandling(async () => {
      const api = this.getAPI();
      const details = await api.getAll({ agentLoopIds: [agentLoopId] });

      let iterations: IterationSummary[] = details.map(d => ({
        iteration: d.iteration,
        startTime: d.startTime,
        endTime: d.endTime,
        status: d.errors && d.errors.length > 0 ? "error" : "completed",
        toolCalls: d.toolCalls?.length ?? 0,
        errors: d.errors?.length ?? 0,
      }));

      if (limit) {
        iterations = iterations.slice(0, limit);
      }

      return iterations;
    }, "listIterations");
  }

  /**
   * Get iteration detail
   */
  async getIterationDetail(
    agentLoopId: ID,
    iterationIndex: number,
  ): Promise<IterationSummary | null> {
    return this.executeWithErrorHandling(async () => {
      const api = this.getAPI();
      const id = `${agentLoopId}:${iterationIndex}`;
      const detail = await api.get(id);

      if (!detail) return null;

      return {
        iteration: detail.iteration,
        startTime: detail.startTime,
        endTime: detail.endTime,
        status: detail.errors && detail.errors.length > 0 ? "error" : "completed",
        toolCalls: detail.toolCalls?.length ?? 0,
        errors: detail.errors?.length ?? 0,
      };
    }, "getIterationDetail");
  }

  /**
   * Get iteration metrics
   */
  async getIterationMetrics(agentLoopId: ID, iterationIndex?: number) {
    return this.executeWithErrorHandling(async () => {
      const api = this.getAPI();

      if (iterationIndex !== undefined) {
        const id = `${agentLoopId}:${iterationIndex}`;
        const detail = await api.get(id);
        if (!detail) return null;

        return {
          iteration: detail.iteration,
          status: detail.errors && detail.errors.length > 0 ? "error" : "completed",
          duration: detail.endTime ? detail.endTime - detail.startTime : undefined,
          toolCalls: detail.toolCalls?.length ?? 0,
          errors: detail.errors?.length ?? 0,
          systemMetrics: detail.systemMetrics,
          llmMetrics: detail.llmMetrics,
          qualityScore: detail.qualityScore,
        };
      }

      const summary = await api.getExtendedHistorySummary(agentLoopId);
      if (!summary) return null;

      return {
        totalIterations: summary.totalIterations,
        status: summary.status,
        totalToolCalls: summary.totalToolCalls,
        totalDuration: summary.totalDuration,
        averageDuration: summary.averageDuration,
        totalErrors: summary.totalErrors,
        errorRate: summary.errorRate,
        averageQualityScore: summary.averageQualityScore,
        totalLLMTokens: summary.totalLLMTokens,
        peakMemoryUsage: summary.peakMemoryUsage,
      };
    }, "getIterationMetrics");
  }

  /**
   * Get error iterations
   */
  async getErrorIterations(agentLoopId: ID): Promise<IterationSummary[]> {
    return this.executeWithErrorHandling(async () => {
      const api = this.getAPI();
      const details = await api.getAll({
        agentLoopIds: [agentLoopId],
        hasErrors: true,
      });

      return details.map(d => ({
        iteration: d.iteration,
        startTime: d.startTime,
        endTime: d.endTime,
        status: "error",
        toolCalls: d.toolCalls?.length ?? 0,
        errors: d.errors?.length ?? 0,
      }));
    }, "getErrorIterations");
  }

  /**
   * Get revision iterations
   */
  async getRevisionIterations(agentLoopId: ID): Promise<IterationSummary[]> {
    return this.executeWithErrorHandling(async () => {
      const api = this.getAPI();
      const details = await api.getAll({
        agentLoopIds: [agentLoopId],
        requiresRevision: true,
      });

      return details.map(d => ({
        iteration: d.iteration,
        startTime: d.startTime,
        endTime: d.endTime,
        status: "revision",
        toolCalls: d.toolCalls?.length ?? 0,
        errors: d.errors?.length ?? 0,
      }));
    }, "getRevisionIterations");
  }

  /**
   * Get iteration decisions
   */
  async getIterationDecisions(
    agentLoopId: ID,
    iterationIndex: number,
  ) {
    return this.executeWithErrorHandling(async () => {
      const api = this.getAPI();
      const id = `${agentLoopId}:${iterationIndex}`;
      const detail = await api.get(id);

      if (!detail || !detail.decisions) return [];

      return detail.decisions;
    }, "getIterationDecisions");
  }

  /**
   * Get iteration errors
   */
  async getIterationErrors(
    agentLoopId: ID,
    iterationIndex: number,
  ) {
    return this.executeWithErrorHandling(async () => {
      const api = this.getAPI();
      const id = `${agentLoopId}:${iterationIndex}`;
      const detail = await api.get(id);

      if (!detail || !detail.errors) return [];

      return detail.errors;
    }, "getIterationErrors");
  }

  /**
   * Analyze decisions across all iterations
   */
  async analyzeDecisions(agentLoopId: ID) {
    return this.executeWithErrorHandling(async () => {
      const api = this.getAPI();
      return await api.analyzeDecisions(agentLoopId);
    }, "analyzeDecisions");
  }

  /**
   * Analyze execution paths across all iterations
   */
  async analyzeExecutionPaths(agentLoopId: ID) {
    return this.executeWithErrorHandling(async () => {
      const api = this.getAPI();
      return await api.analyzePaths(agentLoopId);
    }, "analyzeExecutionPaths");
  }
}
