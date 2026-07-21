/**
 * AgentUserInteractionResourceAPI - Agent User Interaction Resource Management API
 *
 * Extends BaseUserInteractionResourceAPI with agent-specific event recording capabilities.
 * Provides event history tracking, statistics, and export functionality.
 *
 * Features:
 * - Event recording pattern: records all interaction events with timestamps
 * - Event history query and export
 * - Interaction statistics
 * - Handler registration and interaction handling
 */

import {
  BaseUserInteractionResourceAPI,
  type BaseUserInteractionConfig,
  type BaseUserInteractionFilter,
} from "../../shared/resources/user-interaction-base.js";
import { validateRequiredFields } from "../../shared/validation/validation-strategy.js";
import { now } from "@wf-agent/common-utils";
import type {
  UserInteractionHandler,
} from "@wf-agent/types";
import { ConfigurationError } from "@wf-agent/types";
import type { APIDependencyManager } from "../../shared/core/sdk-dependencies.js";
import { validateStringLength, validatePositiveNumber } from "../../shared/validation/validation-strategy.js";

/**
 * Agent User Interaction Configuration
 */
export interface AgentUserInteractionConfig extends BaseUserInteractionConfig {
  /** Execution type */
  executionType?: string;
  /** Timeout (milliseconds) */
  timeout?: number;
  /** Maximum number of retries */
  maxRetries?: number;
  /** Interaction mode */
  mode?: string;
  /** User interaction handler type */
  handlerType?: string;
}

/**
 * Agent User Interaction Filter
 */
export interface AgentUserInteractionFilter extends BaseUserInteractionFilter {
  /** Filter by execution type */
  executionType?: string;
  /** Filter by mode */
  mode?: string;
}

/**
 * Agent User Interaction Event Record
 */
export interface AgentUserInteractionEventRecord {
  /** Event ID */
  id: string;
  /** Config ID */
  configId: string;
  /** Execution ID */
  executionId: string;
  /** Event type (requested, approved, rejected) */
  eventType: string;
  /** Event context */
  context: Record<string, unknown>;
  /** Event timestamp */
  timestamp: number;
  /** Response (if any) */
  response?: unknown;
  /** Response timestamp */
  responseTimestamp?: number;
  /** Response time (milliseconds) */
  responseTime: number | null;
}

/**
 * AgentUserInteractionResourceAPI - Agent User Interaction Resource Management API
 */
export class AgentUserInteractionResourceAPI extends BaseUserInteractionResourceAPI<
  AgentUserInteractionConfig,
  AgentUserInteractionFilter
> {
  private _deps?: APIDependencyManager;
  private eventHistory: AgentUserInteractionEventRecord[] = [];
  private eventIdCounter: number = 0;

  /**
   * Constructor
   * @param deps Optional APIDependencyManager for dependency injection
   */
  constructor(deps?: APIDependencyManager) {
    super();
    this._deps = deps;
  }

  /**
   * Get the dependencies manager
   * @returns APIDependencyManager instance or undefined
   */
  getDependencies(): APIDependencyManager | undefined {
    return this._deps;
  }

  // ============================================================================
  // Implement BaseUserInteractionResourceAPI abstract methods
  // ============================================================================

  protected override async validateResource(
    config: AgentUserInteractionConfig,
    _context?: unknown,
  ): Promise<{ valid: boolean; errors: string[] }> {
    const errors: string[] = [];

    const requiredResult = validateRequiredFields(config, ["id", "name", "type"], "config");
    if (requiredResult.isErr()) {
      errors.push(...requiredResult.unwrapOrElse(err => err.map(e => e.message)));
    }
    const idResult = validateStringLength(config.id, "Config ID", 1, 100);
    if (idResult.isErr()) {
      errors.push(...idResult.unwrapOrElse(err => err.map(e => e.message)));
    }
    const nameResult = validateStringLength(config.name, "Config name", 1, 200);
    if (nameResult.isErr()) {
      errors.push(...nameResult.unwrapOrElse(err => err.map(e => e.message)));
    }
    if (config.timeout !== undefined) {
      const timeoutResult = validatePositiveNumber(config.timeout, "Timeout");
      if (timeoutResult.isErr()) {
        errors.push(...timeoutResult.unwrapOrElse(err => err.map(e => e.message)));
      }
    }

    return { valid: errors.length === 0, errors };
  }

  protected override async validateUpdate(
    updates: Partial<AgentUserInteractionConfig>,
    _context?: unknown,
  ): Promise<{ valid: boolean; errors: string[] }> {
    const errors: string[] = [];

    if (updates.name !== undefined) {
      const nameResult = validateStringLength(updates.name, "Config name", 1, 200);
      if (nameResult.isErr()) {
        errors.push(...nameResult.unwrapOrElse(err => err.map(e => e.message)));
      }
    }
    if (updates.timeout !== undefined) {
      const timeoutResult = validatePositiveNumber(updates.timeout, "Timeout");
      if (timeoutResult.isErr()) {
        errors.push(...timeoutResult.unwrapOrElse(err => err.map(e => e.message)));
      }
    }

    return { valid: errors.length === 0, errors };
  }

  // ============================================================================
  // Agent-specific: Handler Management (with async wrappers for backward compatibility)
  // ============================================================================

  /**
   * Register a user interaction handler
   * @param handler The user interaction handler
   *
   * @deprecated Use {@link registerHandler} instead. This method is kept for
   * backward compatibility and delegates to registerHandler.
   */
  async registerUserInteractionHandler(handler: UserInteractionHandler): Promise<void> {
    this.registerHandler(handler);
  }

  /**
   * Get the registered user interaction handler
   * @returns The handler, or undefined if not registered
   *
   * @deprecated Use {@link getHandler} instead. This method is kept for
   * backward compatibility.
   */
  async getUserInteractionHandler(): Promise<UserInteractionHandler | undefined> {
    return this.getHandler();
  }

  // ============================================================================
  // Agent-specific: Interaction Handling (with event recording)
  // ============================================================================

  /**
   * Handle user interaction with event recording
   * Extends the base handleInteraction with event history recording.
   * @param configId Configuration ID
   * @param executionId Execution ID
   * @param context Interaction context
   * @returns Handler response
   */
  async handleUserInteraction(
    configId: string,
    executionId: string,
    context: Record<string, unknown>,
  ): Promise<unknown> {
    const config = await this.getResource(configId);
    if (!config) {
      throw new ConfigurationError(`User interaction configuration not found: ${configId}`);
    }

    // Record event as requested
    const eventId = this.generateEventId();
    const requestTimestamp = now();

    const event: AgentUserInteractionEventRecord = {
      id: eventId,
      configId,
      executionId,
      eventType: "requested",
      context,
      timestamp: requestTimestamp,
      responseTime: null,
    };

    this.eventHistory.push(event);

    try {
      // If a handler is registered, call it with the structured request
      if (this.userInteractionHandler) {
        const request = {
          interactionId: eventId,
          operationType: (config.type || "TOOL_APPROVAL") as "TOOL_APPROVAL" | "ASK_FOLLOWUP_QUESTION" | "SCRIPT_INTERACTION",
          prompt: config.name,
          timeout: config.timeout ?? 30000,
          metadata: {
            executionId,
            configId,
            ...context,
          },
        };

        const interactionContext = this.createInteractionContext({
          interactionId: eventId,
          operationType: "TOOL_APPROVAL",
          prompt: config.name,
          timeout: config.timeout ?? 30000,
          metadata: { executionId },
        });

        const result = await this.userInteractionHandler.handle(request, interactionContext);

        // Update event with response
        const responseTimestamp = now();
        event.response = result;
        event.responseTimestamp = responseTimestamp;
        event.responseTime = responseTimestamp - requestTimestamp;
        event.eventType = "approved";

        return result;
      }

      // No handler registered - return context as-is (simplified behavior)
      const responseTimestamp = now();
      event.response = context;
      event.responseTimestamp = responseTimestamp;
      event.responseTime = responseTimestamp - requestTimestamp;
      event.eventType = "approved";

      return context;
    } catch (error) {
      // Record failure
      event.eventType = "rejected";
      event.responseTimestamp = now();
      event.responseTime = (event.responseTimestamp ?? now()) - requestTimestamp;
      throw error;
    }
  }

  // ============================================================================
  // Agent-specific: Event History
  // ============================================================================

  /**
   * Get interaction event history
   * @param executionId Optional execution ID to filter by
   * @returns Array of interaction event records
   */
  async getInteractionEventHistory(executionId?: string): Promise<AgentUserInteractionEventRecord[]> {
    if (executionId) {
      return this.eventHistory.filter(event => event.executionId === executionId);
    }
    return [...this.eventHistory];
  }

  /**
   * Get interaction statistics
   * @returns Interaction statistics
   */
  async getInteractionStatistics(): Promise<{
    totalEvents: number;
    byEventType: Record<string, number>;
    averageResponseTime: number;
    maxResponseTime: number;
    minResponseTime: number;
    totalApproved: number;
    totalRejected: number;
  }> {
    const events = this.eventHistory;
    const byEventType: Record<string, number> = {};
    let totalResponseTime = 0;
    let responseTimeCount = 0;
    let maxResponseTime = 0;
    let minResponseTime = Infinity;
    let totalApproved = 0;
    let totalRejected = 0;

    for (const event of events) {
      byEventType[event.eventType] = (byEventType[event.eventType] || 0) + 1;

      if (event.responseTime !== null) {
        totalResponseTime += event.responseTime;
        responseTimeCount++;
        maxResponseTime = Math.max(maxResponseTime, event.responseTime);
        minResponseTime = Math.min(minResponseTime, event.responseTime);
      }

      if (event.eventType === "approved") {
        totalApproved++;
      } else if (event.eventType === "rejected") {
        totalRejected++;
      }
    }

    return {
      totalEvents: events.length,
      byEventType,
      averageResponseTime: responseTimeCount > 0 ? Math.round(totalResponseTime / responseTimeCount) : 0,
      maxResponseTime,
      minResponseTime: responseTimeCount > 0 ? minResponseTime : 0,
      totalApproved,
      totalRejected,
    };
  }

  /**
   * Get configuration interaction history
   * @param configId Configuration ID
   * @returns Array of interaction event records for the specified config
   */
  async getConfigurationInteractionHistory(configId: string): Promise<AgentUserInteractionEventRecord[]> {
    return this.eventHistory.filter(event => event.configId === configId);
  }

  /**
   * Clear interaction history
   * @param olderThanTimestamp Optional timestamp to clear events older than this
   */
  async clearInteractionHistory(olderThanTimestamp?: number): Promise<void> {
    if (olderThanTimestamp !== undefined) {
      this.eventHistory = this.eventHistory.filter(event => event.timestamp > olderThanTimestamp);
    } else {
      this.eventHistory = [];
    }
  }

  /**
   * Export interaction history as JSON string
   * @param executionId Optional execution ID to filter by
   * @returns JSON string of interaction history
   */
  async exportInteractionHistory(executionId?: string): Promise<string> {
    const events = await this.getInteractionEventHistory(executionId);
    return JSON.stringify(events, null, 2);
  }

  /**
   * Generate a unique event ID
   * @returns Unique event ID
   */
  private generateEventId(): string {
    this.eventIdCounter++;
    return `interaction-${now()}-${this.eventIdCounter}`;
  }
}