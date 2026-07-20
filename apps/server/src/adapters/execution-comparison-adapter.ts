/**
 * Execution Comparison Adapter
 * Compare two or more workflow executions.
 */

import { BaseAdapter } from "./base-adapter.js";
import type { ID } from "@wf-agent/types";

export class ExecutionComparisonAdapter extends BaseAdapter {
  override getResourceName(): string {
    return "ExecutionComparison";
  }

  async compareExecutions(exec1Id: ID, exec2Id: ID): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("compareExecutions", { exec1Id, exec2Id });
      const api = this.sdk.executions;
      const [exec1, exec2] = await Promise.all([
        api.get(exec1Id as string),
        api.get(exec2Id as string),
      ]);
      if (!exec1) throw new Error(`Execution not found: ${exec1Id}`);
      if (!exec2) throw new Error(`Execution not found: ${exec2Id}`);

      const e1 = exec1 as any;
      const e2 = exec2 as any;

      return {
        execution1: {
          id: exec1Id,
          status: e1.status,
          duration: e1.duration || e1.elapsedTime || 0,
          iterations: e1.currentIteration || 0,
          errors: e1.errorCount || 0,
        },
        execution2: {
          id: exec2Id,
          status: e2.status,
          duration: e2.duration || e2.elapsedTime || 0,
          iterations: e2.currentIteration || 0,
          errors: e2.errorCount || 0,
        },
        comparison: {
          durationDiff: (e1.duration || 0) - (e2.duration || 0),
          iterationDiff: (e1.currentIteration || 0) - (e2.currentIteration || 0),
          errorDiff: (e1.errorCount || 0) - (e2.errorCount || 0),
        },
      };
    }, "Compare executions");
  }

  async compareRange(execIds: ID[]): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("compareRange", { count: execIds.length });
      const api = this.sdk.executions;
      const executions = await Promise.all(
        execIds.map((id) => api.get(id as string))
      );
      const results = executions.map((exec, i) => {
        const e = exec as any;
        return {
          id: execIds[i],
          status: e?.status || "unknown",
          duration: e?.duration || e?.elapsedTime || 0,
          iterations: e?.currentIteration || 0,
          errors: e?.errorCount || 0,
        };
      });
      return {
        executions: results,
        averageDuration: results.reduce((sum, r) => sum + r.duration, 0) / results.length,
        totalErrors: results.reduce((sum, r) => sum + r.errors, 0),
      };
    }, "Compare execution range");
  }

  async analyzePerformanceTrend(execIds: ID[]): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("analyzePerformanceTrend", { count: execIds.length });
      const api = this.sdk.executions;
      const executions = await Promise.all(
        execIds.map((id) => api.get(id as string))
      );
      const durations = executions.map((e) => (e as any)?.duration || (e as any)?.elapsedTime || 0);
      const avgDuration = durations.reduce((a, b) => a + b, 0) / durations.length;
      const errors = executions.reduce((sum, e) => sum + ((e as any)?.errorCount || 0), 0);

      const trend = durations.length > 1 && durations[durations.length - 1] < durations[0]
        ? "improving"
        : durations.length > 1 && durations[durations.length - 1] > durations[0]
          ? "degrading"
          : "stable";

      return {
        trend,
        totalExecutions: execIds.length,
        avgDuration,
        avgErrors: errors / execIds.length,
        durations,
      };
    }, "Analyze performance trend");
  }
}