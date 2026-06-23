/**
 * Execution Progress Analysis
 * Real-time execution progress tracking with time estimation
 */

import type { ID } from "@wf-agent/types";
import { EventEmitter } from "node:events";

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
  status: 'running' | 'completed' | 'failed' | 'paused';
}

export type ProgressListener = (metrics: ProgressMetrics) => void;

export enum ProgressEventType {
  PROGRESS = 'progress',
  COMPLETE = 'complete',
  FAILED = 'failed',
  PAUSED = 'paused',
  RESUMED = 'resumed',
}

/**
 * Progress tracker for real-time monitoring
 */
export class ProgressTracker {
  private executionId: ID;
  private sdk: any;
  private emitter: EventEmitter = new EventEmitter();
  private currentMetrics: ProgressMetrics | null = null;
  private isTracking: boolean = false;
  private pollInterval: number = 1000;
  private pollTimer: NodeJS.Timeout | null = null;

  constructor(executionId: ID, sdk: any, pollInterval?: number) {
    this.executionId = executionId;
    this.sdk = sdk;
    if (pollInterval) {
      this.pollInterval = pollInterval;
    }
  }

  async start(): Promise<void> {
    if (this.isTracking) return;
    this.isTracking = true;
    await this.updateMetrics();
    this.pollTimer = setInterval(async () => {
      try {
        await this.updateMetrics();
      } catch (error) {
        // Silently fail
      }
    }, this.pollInterval);
  }

  async stop(): Promise<void> {
    if (!this.isTracking) return;
    this.isTracking = false;
    if (this.pollTimer) {
      clearInterval(this.pollTimer);
      this.pollTimer = null;
    }
  }

  on(event: ProgressEventType, listener: ProgressListener | (() => void)): () => void {
    this.emitter.on(event, listener as any);
    return () => {
      this.emitter.off(event, listener as any);
    };
  }

  once(event: ProgressEventType, listener: ProgressListener | (() => void)): void {
    this.emitter.once(event, listener as any);
  }

  getMetrics(): ProgressMetrics | null {
    return this.currentMetrics;
  }

  getProgressPercentage(): number {
    return this.currentMetrics?.progressPercentage || 0;
  }

  estimateRemainingTime(): number {
    return this.currentMetrics?.estimatedRemainingTime || -1;
  }

  estimateCompletionTime(): Date | null {
    return this.currentMetrics?.estimatedCompletionTime || null;
  }

  getStatus(): string {
    return this.currentMetrics?.status || 'unknown';
  }

  private async updateMetrics(): Promise<void> {
    try {
      const factory = this.sdk.getFactory();
      const deps = factory?.getDependencies?.();
      if (!deps) return;

      const registry = deps.getAgentLoopRegistry?.();
      if (!registry) return;

      const entity = await registry.get(this.executionId);
      if (!entity) return;

      const state = entity.state;
      const now = Date.now();
      const startTime = state.startTime || now;
      const elapsedTime = now - startTime;

      const perfApi = deps.getAgentPerformanceAnalysisAPI?.();
      const profile = perfApi ? await perfApi.analyzePerformance(this.executionId) : null;

      const totalIterations = state.currentIteration;
      const currentIteration = state.currentIteration;

      const iterationsPerSecond = totalIterations > 0
        ? totalIterations / (elapsedTime / 1000)
        : 0;
      const toolCallsPerSecond = profile && profile.totalToolCalls > 0
        ? profile.totalToolCalls / (elapsedTime / 1000)
        : 0;

      let estimatedRemainingTime = -1;
      let estimatedTotalTime = elapsedTime;
      let confidence = 0;

      if (profile && totalIterations > 0 && iterationsPerSecond > 0) {
        const avgIterationDuration = profile.summary.avgIterationDuration;
        const remainingIterations = Math.max(
          0,
          (profile.totalIterations || totalIterations) - currentIteration
        );
        estimatedRemainingTime = Math.round(remainingIterations * avgIterationDuration);
        estimatedTotalTime = elapsedTime + estimatedRemainingTime;
        confidence = 0.8;
      }

      let estimatedCompletionTime: Date | null = null;
      if (estimatedRemainingTime > 0) {
        estimatedCompletionTime = new Date(now + estimatedRemainingTime);
      }

      const metrics: ProgressMetrics = {
        executionId: this.executionId,
        iteration: currentIteration,
        totalIterations: profile?.totalIterations || totalIterations,
        progressPercentage: totalIterations > 0
          ? (currentIteration / totalIterations) * 100
          : 0,
        elapsedTime,
        estimatedRemainingTime,
        estimatedTotalTime,
        estimatedCompletionTime,
        confidence,
        iterationsPerSecond: Math.round(iterationsPerSecond * 100) / 100,
        toolCallsPerSecond: Math.round(toolCallsPerSecond * 100) / 100,
        status: entity.getStatus() as any,
      };

      const prevMetrics = this.currentMetrics;
      this.currentMetrics = metrics;

      if (prevMetrics == null || prevMetrics.iteration !== metrics.iteration) {
        this.emitter.emit(ProgressEventType.PROGRESS, metrics);
      }

      if (metrics.status === 'completed' && prevMetrics?.status !== 'completed') {
        this.emitter.emit(ProgressEventType.COMPLETE, metrics);
      }

      if (metrics.status === 'failed' && prevMetrics?.status !== 'failed') {
        this.emitter.emit(ProgressEventType.FAILED, metrics);
      }
    } catch (error) {
      // Silently fail
    }
  }
}

/**
 * Progress Analysis API
 */
export class ProgressAnalysis {
  constructor(private sdk: any) {}

  createTracker(executionId: ID, pollInterval?: number): ProgressTracker {
    return new ProgressTracker(executionId, this.sdk, pollInterval);
  }

  async getProgress(executionId: ID): Promise<ProgressMetrics | null> {
    const tracker = this.createTracker(executionId);
    await tracker.start();
    const metrics = tracker.getMetrics();
    await tracker.stop();
    return metrics;
  }

  formatProgress(metrics: ProgressMetrics): string {
    const percent = metrics.progressPercentage.toFixed(1);
    const eta = metrics.estimatedCompletionTime
      ? metrics.estimatedCompletionTime.toLocaleTimeString()
      : 'unknown';

    return (
      `Progress: ${percent}% | ` +
      `Iteration ${metrics.iteration}/${metrics.totalIterations} | ` +
      `ETA: ${eta}`
    );
  }
}
