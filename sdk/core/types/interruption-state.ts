/**
 * Interruption State
 * Provides unified management of interrupt status and operations
 *
 * Responsibilities:
 * - Manages interrupt status (PAUSE/STOP)
 * - Supplies the AbortSignal for deep interrupts
 * - Coordinates interrupt requests and recovery operations
 *
 * Design Principles:
 * - Single Responsibility Principle: Responsible only for interrupt status management
 * - Encapsulation: Hides internal implementation details
 * - Execution Safety: Ensures atomicity of state changes
 * - Unified Use of AbortSignal as the primary interrupt mechanism
 * - Portability: Can be shared by both the Graph module and the Agent module
 */

import { InterruptedException, WorkflowExecutionInterruptedException } from "@wf-agent/types";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "InterruptionState" });

/**
 * Interrupt types
 */
export type InterruptionType = "PAUSE" | "STOP" | null;

/**
 * Interrupt exception information
 */
export interface InterruptionInfo {
  /** Interrupt Types */
  type: InterruptionType;
  /** Interrupt message */
  message: string;
  /** Context ID (such as execution ID, session ID, etc.) */
  contextId?: string;
  /** Node ID (optional) */
  nodeId?: string;
}

/**
 * Interruption State Configuration
 */
export interface InterruptionStateConfig {
  /** Context ID (such as execution ID, session ID, etc.) */
  contextId: string;
  /** Node ID (optional) */
  nodeId?: string;
  /** Custom Interrupt Exception Creation Function */
  createInterruptionError?: (info: InterruptionInfo) => InterruptedException;
}

/**
 * Interruption State
 *
 * A general-purpose interrupt management component that supports shared use by both the Graph module and the Agent module.
 */
export class InterruptionState {
  private abortController: AbortController = new AbortController();
  private interruptionType: InterruptionType = null;
  private contextId: string;
  private nodeId: string;
  private createInterruptionError?: (info: InterruptionInfo) => InterruptedException;

  /**
   * Constructor
   * @param config Configuration options
   */
  constructor(config: InterruptionStateConfig);

  constructor(configOrThreadId: InterruptionStateConfig | string, nodeId?: string) {
    if (typeof configOrThreadId === "string") {
      // Backward compatibility: The old form of the constructor
      this.contextId = configOrThreadId;
      this.nodeId = nodeId ?? "";
    } else {
      // New configuration object format
      this.contextId = configOrThreadId.contextId;
      this.nodeId = configOrThreadId.nodeId ?? "";
      this.createInterruptionError = configOrThreadId.createInterruptionError;
    }
  }

  /**
   * Request to pause
   */
  requestPause(): void {
    if (this.interruptionType === "PAUSE") {
      return; // It is already in a paused state.
    }

    logger.info("Execution pause requested", { contextId: this.contextId, nodeId: this.nodeId });
    this.interruptionType = "PAUSE";
    const error = this.createError("Execution paused", "PAUSE");
    this.abortController.abort(error);
  }

  /**
   * Request to stop
   */
  requestStop(): void {
    if (this.interruptionType === "STOP") {
      return; // It is already in a stopped state.
    }

    logger.info("Execution stop requested", { contextId: this.contextId, nodeId: this.nodeId });
    this.interruptionType = "STOP";
    const error = this.createError("Execution stopped", "STOP");
    this.abortController.abort(error);
  }

  /**
   * Resume execution
   */
  resume(): void {
    logger.info("Execution resumed", { contextId: this.contextId, nodeId: this.nodeId });
    this.interruptionType = null;
    // Reset the AbortController
    this.abortController = new AbortController();
  }

  /**
   * Get the interrupt type
   */
  getInterruptionType(): InterruptionType {
    return this.interruptionType;
  }

  /**
   * Get the AbortSignal
   */
  getAbortSignal(): AbortSignal {
    return this.abortController.signal;
  }

  /**
   * Check if it has been aborted.
   */
  isAborted(): boolean {
    return this.abortController.signal.aborted;
  }

  /**
   * Check whether it should be paused.
   */
  shouldPause(): boolean {
    return this.interruptionType === "PAUSE";
  }

  /**
   * Check whether it should be stopped.
   */
  shouldStop(): boolean {
    return this.interruptionType === "STOP";
  }

  /**
   * Get the reason for the termination.
   */
  getAbortReason(): Error | undefined {
    return this.abortController.signal.reason as Error | undefined;
  }

  /**
   * Update the current node ID
   */
  updateNodeId(nodeId: string): void {
    this.nodeId = nodeId;
  }

  /**
   * Get the context ID
   */
  getContextId(): string {
    return this.contextId;
  }

  /**
   * Get the node ID
   */
  getNodeId(): string {
    return this.nodeId;
  }

  /**
   * Create an interrupt error
   */
  private createError(message: string, type: "PAUSE" | "STOP"): InterruptedException {
    const info: InterruptionInfo = {
      type,
      message,
      contextId: this.contextId,
      nodeId: this.nodeId,
    };

    if (this.createInterruptionError) {
      return this.createInterruptionError(info);
    }

    // The default is to use WorkflowExecutionInterruptedException (for backward compatibility).
    return new WorkflowExecutionInterruptedException(message, type, this.contextId, this.nodeId);
  }
}
