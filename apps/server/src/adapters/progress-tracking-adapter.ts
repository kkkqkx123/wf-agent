/**
 * Progress Tracking Adapter
 * Monitor real-time execution progress.
 */

import { BaseAdapter } from "./base-adapter.js";
import type { ID } from "@wf-agent/types";

export class ProgressTrackingAdapter extends BaseAdapter {
  override getResourceName(): string {
    return "ProgressTracking";
  }

  async getProgress(executionId: ID): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("getProgress", { executionId });
      const api = this.sdk.executions;
      const execution = await api.get(executionId as string);
      if (!execution) {
        throw new Error(`Execution not found: ${executionId}`);
      }
      const exec = execution as any;
      return {
        executionId,
        status: exec.status || "unknown",
        progressPercentage: exec.progressPercentage || 0,
        iteration: exec.currentIteration || 0,
        totalIterations: exec.totalIterations || 0,
        elapsedTime: exec.elapsedTime || 0,
        estimatedRemainingTime: exec.estimatedRemainingTime || 0,
        estimatedCompletionTime: exec.estimatedCompletionTime
          ? new Date(exec.estimatedCompletionTime).toISOString()
          : null,
        currentStep: exec.currentNodeId || null,
        totalSteps: exec.totalNodes || 0,
      };
    }, "Get progress");
  }

  async getExecutionStatus(executionId: ID): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("getExecutionStatus", { executionId });
      const api = this.sdk.executions;
      const execution = await api.get(executionId as string);
      if (!execution) {
        throw new Error(`Execution not found: ${executionId}`);
      }
      const exec = execution as any;
      return {
        id: executionId,
        status: exec.status || "unknown",
        progress: `${exec.progressPercentage || 0}%`,
        iteration: `${exec.currentIteration || 0}/${exec.totalIterations || 0}`,
        elapsed: `${((exec.elapsedTime || 0) / 1000).toFixed(1)}s`,
        remaining: exec.estimatedRemainingTime > 0
          ? `${(exec.estimatedRemainingTime / 1000).toFixed(1)}s`
          : "N/A",
        completionTime: exec.estimatedCompletionTime
          ? new Date(exec.estimatedCompletionTime).toLocaleTimeString()
          : "N/A",
      };
    }, "Get execution status");
  }
}