/**
 * Iteration Analysis Adapter
 * Analyze agent loop iterations.
 */

import { BaseAdapter } from "./base-adapter.js";
import type { ID } from "@wf-agent/types";

export class IterationAnalysisAdapter extends BaseAdapter {
  override getResourceName(): string {
    return "IterationAnalysis";
  }

  private getIterationAPI() {
    return this.sdk.getFactory().createAgentLoopIterationAPI();
  }

  async getIterationHistorySummary(agentLoopId: ID): Promise<Record<string, any> | null> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("getIterationHistorySummary", { agentLoopId });
      const api = this.getIterationAPI();
      const summary = await api.getExtendedHistorySummary(agentLoopId);
      return summary as any;
    }, "Get iteration history summary");
  }

  async listIterations(agentLoopId: ID, limit?: number): Promise<Record<string, any>[]> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("listIterations", { agentLoopId, limit });
      const api = this.getIterationAPI();
      const filter: any = { agentLoopIds: [agentLoopId] };
      const iterations = await api.getAll(filter);
      const sliced = limit ? iterations.slice(0, limit) : iterations;
      return sliced.map((iter: any) => ({
        iteration: iter.iteration,
        status: iter.status,
        toolCalls: iter.toolCalls?.length || 0,
        errors: iter.errors?.length || 0,
        timestamp: iter.startTime,
      }));
    }, "List iterations");
  }

  async analyzeDecisions(agentLoopId: ID): Promise<Record<string, any> | null> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("analyzeDecisions", { agentLoopId });
      const api = this.getIterationAPI();
      const analysis = await api.analyzeDecisions(agentLoopId);
      return analysis as any;
    }, "Analyze decisions");
  }

  async analyzeExecutionPaths(agentLoopId: ID): Promise<Record<string, any> | null> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("analyzeExecutionPaths", { agentLoopId });
      const api = this.getIterationAPI();
      const analysis = await api.analyzePaths(agentLoopId);
      return analysis as any;
    }, "Analyze execution paths");
  }

  async getIterationMetrics(agentLoopId: ID, _iterationIndex?: number): Promise<Record<string, any> | null> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("getIterationMetrics", { agentLoopId, _iterationIndex });
      const api = this.getIterationAPI();
      const summary = await api.getExtendedHistorySummary(agentLoopId);
      return summary as any;
    }, "Get iteration metrics");
  }
}