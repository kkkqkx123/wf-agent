/**
 * Pause Timeout Manager
 *
 * Manages timeout for paused workflow executions.
 *
 * This manager now uses the TimeoutManager from the execution entity to ensure consistency
 * across the SDK and better resource management.
 */

import type { WorkflowExecutionRegistry } from "../../stores/workflow-execution-registry.js";
import type { EventRegistry } from "../../../shared/registry/event-registry.js";
import type { BaseEvent } from "@wf-agent/types";
import { createContextualLogger } from "../../../utils/contextual-logger.js";
import { buildWorkflowExecutionCancelledEvent } from "../../../shared/utils/event/builders/workflow-execution-events.js";
import { emit } from "../../../shared/utils/event/emit-event.js";
import type { TimeoutHandle } from "../../../shared/types/timeout.js";

const logger = createContextualLogger({ component: "pause-timeout-manager" });

/**
 * Pause timeout configuration
 */
export interface PauseTimeoutConfig {
  /** Maximum pause duration in milliseconds (default: 24 hours) */
  maxPauseDuration: number;
  /** Warning threshold before timeout (default: 1 hour before timeout) */
  warningThreshold: number;
}

/**
 * Default pause timeout configuration
 */
const DEFAULT_CONFIG: PauseTimeoutConfig = {
  maxPauseDuration: 24 * 60 * 60 * 1000, // 24 hours
  warningThreshold: 60 * 60 * 1000, // 1 hour
};

/**
 * Pause timeout tracking entry
 */
interface PauseTimeoutEntry {
  executionId: string;
  pausedAt: number;
  warningEmitted: boolean;
  timeoutHandle?: TimeoutHandle;
}

/**
 * Pause Timeout Manager
 * Monitors paused workflows and cancels them if they exceed the timeout
 */
export class PauseTimeoutManager {
  private entries: Map<string, PauseTimeoutEntry> = new Map();
  private config: PauseTimeoutConfig;

  constructor(
    private registry: WorkflowExecutionRegistry,
    private eventManager: EventRegistry,
    config?: Partial<PauseTimeoutConfig>,
  ) {
    this.config = { ...DEFAULT_CONFIG, ...config };
  }

  /**
   * Start monitoring a paused workflow execution
   */
  startMonitoring(executionId: string): void {
    // Clear any existing monitoring
    this.stopMonitoring(executionId);

    const pausedAt = Date.now();

    // Get the workflow execution entity and use its timeoutManager
    const workflowExecutionEntity = this.registry.get(executionId);
    if (!workflowExecutionEntity) {
      logger.warn("Workflow execution not found for timeout monitoring", { executionId });
      return;
    }

    const timeoutManager = workflowExecutionEntity.timeoutManager;
    const interruptionState = workflowExecutionEntity.getInterruptionState();

    const handle = timeoutManager.register({
      id: `pause-${executionId}`,
      duration: this.config.maxPauseDuration,
      onTimeout: async () => {
        await this.handleTimeout(executionId);
      },
      warningThreshold: this.config.warningThreshold,
      onWarning: async () => {
        await this.emitWarning(executionId);
      },
      interruptionState, // Bind to interruption state for automatic cancellation
      tag: "workflow-pause",
      metadata: {
        executionId,
        pausedAt,
        maxPauseDuration: this.config.maxPauseDuration,
        warningThreshold: this.config.warningThreshold,
      },
    });

    const entry: PauseTimeoutEntry = {
      executionId,
      pausedAt,
      warningEmitted: false,
      timeoutHandle: handle,
    };

    this.entries.set(executionId, entry);

    logger.debug("Started monitoring paused workflow", {
      executionId,
      maxPauseDuration: this.config.maxPauseDuration,
      warningThreshold: this.config.warningThreshold,
    });
  }

  /**
   * Stop monitoring a workflow execution (when resumed or cancelled)
   */
  stopMonitoring(executionId: string): void {
    const entry = this.entries.get(executionId);
    if (entry) {
      // Cancel the timeout
      if (entry.timeoutHandle) {
        entry.timeoutHandle.cancel();
      }

      this.entries.delete(executionId);
      logger.debug("Stopped monitoring paused workflow", { executionId });
    }
  }

  /**
   * Check if a workflow execution is being monitored
   */
  isMonitoring(executionId: string): boolean {
    return this.entries.has(executionId);
  }

  /**
   * Get pause duration for a workflow execution
   */
  getPauseDuration(executionId: string): number | null {
    const entry = this.entries.get(executionId);
    if (!entry) {
      return null;
    }
    return Date.now() - entry.pausedAt;
  }

  /**
   * Emit warning event when approaching timeout
   */
  private async emitWarning(executionId: string): Promise<void> {
    const entry = this.entries.get(executionId);
    if (!entry || entry.warningEmitted) {
      return;
    }

    const workflowExecutionEntity = this.registry.get(executionId);
    if (!workflowExecutionEntity) {
      logger.warn("Workflow execution not found for warning", { executionId });
      this.stopMonitoring(executionId);
      return;
    }

    const pauseDuration = Date.now() - entry.pausedAt;
    const remainingTime = this.config.maxPauseDuration - pauseDuration;

    logger.warn("Workflow execution approaching pause timeout", {
      executionId,
      pauseDuration,
      remainingTime,
      maxPauseDuration: this.config.maxPauseDuration,
    });

    // Emit warning event
    try {
      const warningEvent = {
        id: crypto.randomUUID(),
        type: "WORKFLOW_EXECUTION_PAUSE_TIMEOUT_WARNING",
        timestamp: Date.now(),
        executionId,
        workflowId: workflowExecutionEntity.getWorkflowId(),
        pauseDuration,
        remainingTime,
        maxPauseDuration: this.config.maxPauseDuration,
        message: `Workflow execution will be automatically cancelled in ${Math.round(remainingTime / 1000)} seconds if not resumed`,
      } as unknown as BaseEvent;
      await this.eventManager.emit(warningEvent);
    } catch (error) {
      logger.error("Failed to emit pause timeout warning", {
        executionId,
        error: error instanceof Error ? error.message : String(error),
      });
    }

    entry.warningEmitted = true;
  }

  /**
   * Handle timeout - cancel the workflow execution
   */
  private async handleTimeout(executionId: string): Promise<void> {
    const entry = this.entries.get(executionId);
    if (!entry) {
      return;
    }

    const workflowExecutionEntity = this.registry.get(executionId);
    if (!workflowExecutionEntity) {
      logger.warn("Workflow execution not found for timeout", { executionId });
      this.stopMonitoring(executionId);
      return;
    }

    const pauseDuration = Date.now() - entry.pausedAt;

    logger.info("Workflow execution pause timeout exceeded, cancelling", {
      executionId,
      pauseDuration,
      maxPauseDuration: this.config.maxPauseDuration,
    });

    try {
      // Update status to CANCELLED
      workflowExecutionEntity.state.cancel();

      // Emit cancellation event
      const cancelledEvent = buildWorkflowExecutionCancelledEvent(
        workflowExecutionEntity,
        "pause_timeout",
        {
          nodeId: workflowExecutionEntity.getCurrentNodeId(),
          completedNodes: workflowExecutionEntity.getNodeResults().length,
          pauseDuration,
          maxPauseDuration: this.config.maxPauseDuration,
        },
      );
      await emit(this.eventManager, cancelledEvent);

      logger.info("Workflow execution cancelled due to pause timeout", {
        executionId,
        pauseDuration,
      });
    } catch (error) {
      logger.error("Failed to cancel workflow execution on timeout", {
        executionId,
        error: error instanceof Error ? error.message : String(error),
      });
    } finally {
      this.stopMonitoring(executionId);
    }
  }

  /**
   * Clean up all monitoring
   */
  cleanup(): void {
    // Cancel all timeouts
    for (const [executionId] of this.entries) {
      const entry = this.entries.get(executionId);
      if (entry?.timeoutHandle) {
        entry.timeoutHandle.cancel();
      }
    }

    this.entries.clear();
  }
}
