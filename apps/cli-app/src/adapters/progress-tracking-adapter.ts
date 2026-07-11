/**
 * Progress Tracking Adapter
 * Wraps ProgressTracker/ProgressAnalysis for CLI usage
 */

import { BaseAdapter } from "./base-adapter.js";
import type { ID } from "@wf-agent/types";

export interface ProgressMetrics {
  executionId: ID;
  iteration: number;
  totalIterations: number;
  progressPercentage: number;
  elapsedTime: number;
  estimatedRemainingTime: number;
  estimatedTotalTime: number;
  estimatedCompletionTime: Date | null;
  confidence: number;
  iterationsPerSecond: number;
  toolCallsPerSecond: number;
  status: "running" | "completed" | "failed" | "paused";
}

export class ProgressTrackingAdapter extends BaseAdapter {
  constructor() {
    super();
  }

  /**
   * Get current progress metrics for an execution
   */
  async getProgress(executionId: ID): Promise<ProgressMetrics> {
    return this.executeWithErrorHandling(async () => {
      const factory = this.sdk.getFactory();
      const deps = factory?.getDependencies?.();
      if (!deps) throw new Error("SDK dependencies not available");

      const registry = deps.getAgentLoopRegistry?.();
      const perfApi = deps.getAgentPerformanceAnalysisAPI?.();

      if (!registry) throw new Error("Agent loop registry not available");

      const entity = await registry.get(executionId);
      if (!entity) throw new Error(`Execution not found: ${executionId}`);

      const state = entity.state;
      const now = Date.now();
      const startTime = state.startTime || now;
      const elapsedTime = now - startTime;

      let totalIterations = (state as any).totalIterations || (state as any).targetIterations || state.currentIteration || 0;
      const currentIteration = state.currentIteration || 0;

      let iterationsPerSecond = 0;
      let toolCallsPerSecond = 0;
      let estimatedRemainingTime = -1;
      let estimatedTotalTime = elapsedTime;
      let confidence = 0;
      let status: "running" | "completed" | "failed" | "paused" = "running";

      if (elapsedTime > 0) {
        iterationsPerSecond = currentIteration / (elapsedTime / 1000);
      }

      if (perfApi) {
        try {
          const profile = await perfApi.analyzePerformance(executionId);
          if (profile) {
            toolCallsPerSecond = profile.totalToolCalls
              ? profile.totalToolCalls / (elapsedTime / 1000)
              : 0;

            if (profile.totalIterations) {
              totalIterations = profile.totalIterations;
            }

            if (profile.summary?.avgIterationDuration) {
              const avgIterationDuration = profile.summary.avgIterationDuration;
              const remainingIterations = Math.max(
                0,
                totalIterations - currentIteration
              );
              estimatedRemainingTime = Math.round(
                remainingIterations * avgIterationDuration
              );
              estimatedTotalTime = elapsedTime + estimatedRemainingTime;
              confidence = 0.8;
            }
          }
        } catch {
          // Silently fail and use basic estimation
        }
      }

      // Determine status based on execution state
      if (state.status === "COMPLETED" || (state as any).status === "SUCCESS") {
        status = "completed";
        estimatedRemainingTime = 0;
        confidence = 1;
      } else if (state.status === "FAILED" || (state as any).status === "ERROR") {
        status = "failed";
        confidence = 1;
      } else if (state.status === "PAUSED") {
        status = "paused";
      }

      const progressPercentage =
        totalIterations > 0
          ? Math.min(100, (currentIteration / totalIterations) * 100)
          : 0;

      const completionTime =
        estimatedTotalTime > 0
          ? new Date(startTime + estimatedTotalTime)
          : null;

      const metrics: ProgressMetrics = {
        executionId,
        iteration: currentIteration,
        totalIterations,
        progressPercentage: Math.round(progressPercentage * 100) / 100,
        elapsedTime: Math.round(elapsedTime),
        estimatedRemainingTime: Math.max(0, estimatedRemainingTime),
        estimatedTotalTime: Math.round(estimatedTotalTime),
        estimatedCompletionTime: completionTime,
        confidence: Math.round(confidence * 100) / 100,
        iterationsPerSecond: Math.round(iterationsPerSecond * 100) / 100,
        toolCallsPerSecond: Math.round(toolCallsPerSecond * 100) / 100,
        status,
      };

      this.logOperation(
        `Retrieved progress: ${progressPercentage.toFixed(1)}% complete`
      );
      return metrics;
    }, "Get execution progress");
  }

  /**
   * Watch execution progress until completion
   */
  async watchProgress(
    executionId: ID,
    onUpdate?: (metrics: ProgressMetrics) => void,
    pollInterval: number = 1000
  ): Promise<ProgressMetrics> {
    return this.executeWithErrorHandling(async () => {
      let lastStatus = "";

      return new Promise((resolve, reject) => {
        const pollTimer = setInterval(async () => {
          try {
            const metrics = await this.getProgress(executionId);

            if (lastStatus !== metrics.status) {
              this.logOperation(
                `Status changed: ${lastStatus || "unknown"} → ${metrics.status}`
              );
              lastStatus = metrics.status;
            }

            if (onUpdate) {
              onUpdate(metrics);
            }

            if (
              metrics.status === "completed" ||
              metrics.status === "failed"
            ) {
              clearInterval(pollTimer);
              resolve(metrics);
            }
          } catch (error) {
            clearInterval(pollTimer);
            reject(error);
          }
        }, pollInterval);
      });
    }, "Watch execution progress");
  }

  /**
   * Format progress metrics for display
   */
  formatProgressBar(metrics: ProgressMetrics): string {
    const barLength = 30;
    const filledLength = Math.round(
      (metrics.progressPercentage / 100) * barLength
    );
    const emptyLength = barLength - filledLength;

    const bar = "█".repeat(filledLength) + "░".repeat(emptyLength);
    const percentage = metrics.progressPercentage.toFixed(1);

    let status = "";
    if (metrics.status === "completed") {
      status = "✅ Complete";
    } else if (metrics.status === "failed") {
      status = "❌ Failed";
    } else if (metrics.status === "paused") {
      status = "⏸️  Paused";
    } else {
      status = "▶️  Running";
    }

    return `[${bar}] ${percentage}% (${metrics.iteration}/${metrics.totalIterations}) ${status}`;
  }

  /**
   * Format detailed progress metrics
   */
  formatProgressMetrics(metrics: ProgressMetrics): string {
    const lines: string[] = [];

    lines.push("");
    lines.push("📊 Execution Progress Metrics");
    lines.push("");

    lines.push(`Execution ID: ${metrics.executionId}`);
    lines.push(
      `Status: ${metrics.status === "completed" ? "✅ Complete" : metrics.status === "failed" ? "❌ Failed" : metrics.status === "paused" ? "⏸️  Paused" : "▶️  Running"}`
    );
    lines.push("");

    lines.push("Progress:");
    lines.push(`  ${this.formatProgressBar(metrics)}`);
    lines.push("");

    lines.push("Timing:");
    lines.push(`  Elapsed: ${this.formatTime(metrics.elapsedTime)}`);
    if (metrics.estimatedRemainingTime > 0) {
      lines.push(
        `  Remaining: ${this.formatTime(metrics.estimatedRemainingTime)}`
      );
    }
    lines.push(`  Total Estimated: ${this.formatTime(metrics.estimatedTotalTime)}`);
    if (metrics.estimatedCompletionTime) {
      lines.push(
        `  Est. Completion: ${metrics.estimatedCompletionTime.toLocaleTimeString()}`
      );
    }
    lines.push("");

    lines.push("Performance:");
    lines.push(
      `  Iterations/sec: ${metrics.iterationsPerSecond.toFixed(2)}`
    );
    lines.push(
      `  Tool Calls/sec: ${metrics.toolCallsPerSecond.toFixed(2)}`
    );
    lines.push(`  Confidence: ${(metrics.confidence * 100).toFixed(0)}%`);
    lines.push("");

    return lines.join("\n");
  }

  /**
   * Format time in human-readable format
   */
  private formatTime(milliseconds: number): string {
    if (milliseconds < 0) return "Unknown";

    const seconds = Math.floor(milliseconds / 1000);
    const minutes = Math.floor(seconds / 60);
    const hours = Math.floor(minutes / 60);
    const days = Math.floor(hours / 24);

    if (days > 0) {
      return `${days}d ${hours % 24}h ${minutes % 60}m`;
    }
    if (hours > 0) {
      return `${hours}h ${minutes % 60}m ${seconds % 60}s`;
    }
    if (minutes > 0) {
      return `${minutes}m ${seconds % 60}s`;
    }
    return `${seconds}s`;
  }

  /**
   * Estimate time remaining based on current metrics
   */
  estimateTimeRemaining(metrics: ProgressMetrics): string {
    if (
      metrics.estimatedRemainingTime < 0 ||
      metrics.status !== "running"
    ) {
      return "Unknown";
    }
    return this.formatTime(metrics.estimatedRemainingTime);
  }

  /**
   * Estimate completion time
   */
  estimateCompletionTime(metrics: ProgressMetrics): string {
    if (!metrics.estimatedCompletionTime) {
      return "Unknown";
    }
    return metrics.estimatedCompletionTime.toLocaleTimeString();
  }
}
