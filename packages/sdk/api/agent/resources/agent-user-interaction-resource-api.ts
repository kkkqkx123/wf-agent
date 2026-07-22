/**
 * AgentUserInteractionResourceAPI - Agent User Interaction Resource Management API
 *
 * Extends BaseUserInteractionResourceAPI with agent-specific interaction handling.
 * Uses the base class's optional event recording feature for event history tracking.
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
  type UserInteractionEventRecord,
} from "../../shared/resources/user-interaction-base.js";
import { validateRequiredFields } from "../../shared/validation/validation-strategy.js";
import { now } from "@wf-agent/common-utils";
import type {
  UserInteractionHandler,
  ToolApprovalRequestedEvent,
  FollowupQuestionRequestedEvent,
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
 * Alias for the shared UserInteractionEventRecord from the base class.
 */
export type AgentUserInteractionEventRecord = UserInteractionEventRecord;

/**
 * AgentUserInteractionResourceAPI - Agent User Interaction Resource Management API
 */
export class AgentUserInteractionResourceAPI extends BaseUserInteractionResourceAPI<
  AgentUserInteractionConfig,
  AgentUserInteractionFilter
> {
  private _deps?: APIDependencyManager;

  /**
   * Constructor
   * @param deps Optional APIDependencyManager for dependency injection
   */
  constructor(deps?: APIDependencyManager) {
    super();
    this._deps = deps;
    // Enable event recording by default for agent user interactions
    this.eventRecordingEnabled = true;
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

    // Record event as requested using base class method
    const event = this.recordInteractionEvent(configId, executionId, "requested", context);

    try {
      // If a handler is registered, call it with the structured request
      if (this.userInteractionHandler) {
        const request = {
          interactionId: event.id,
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
          interactionId: event.id,
          operationType: "TOOL_APPROVAL",
          prompt: config.name,
          timeout: config.timeout ?? 30000,
          metadata: { executionId },
        });

        const result = await this.userInteractionHandler.handle(request, interactionContext);

        // Update event with response using base class method
        this.completeInteractionEvent(event, result);
        event.eventType = "approved";

        return result;
      }

      // No handler registered - return context as-is (simplified behavior)
      this.completeInteractionEvent(event, context);
      event.eventType = "approved";

      return context;
    } catch (error) {
      // Record failure
      event.eventType = "rejected";
      event.responseTimestamp = now();
      event.responseTime = (event.responseTimestamp ?? now()) - event.timestamp;
      throw error;
    }
  }

  // ============================================================================
  // Agent-specific: Event History
  // ============================================================================

  /**
   * Get configuration interaction history
   * @param configId Configuration ID
   * @returns Array of interaction event records for the specified config
   */
  async getConfigurationInteractionHistory(configId: string): Promise<AgentUserInteractionEventRecord[]> {
    return (await this.getInteractionEventHistory()).filter(event => event.configId === configId);
  }

  // ============================================================================
  // Agent-specific: Event Subscriptions
  // ============================================================================

  /**
   * Subscribe to tool approval request events
   * @param executionId Execution ID (required)
   * @param listener Event listener
   * @returns Unsubscribe function
   */
  onToolApprovalRequested(
    executionId: string,
    listener: (event: ToolApprovalRequestedEvent) => void,
  ): () => void {
    const emitter = this._deps!.getEventManager().getEmitter(executionId);
    return emitter.on("TOOL_APPROVAL_REQUESTED", listener);
  }

  /**
   * Subscribe to follow-up question request events
   * @param executionId Execution ID (required)
   * @param listener Event listener
   * @returns Unsubscribe function
   */
  onFollowupQuestionRequested(
    executionId: string,
    listener: (event: FollowupQuestionRequestedEvent) => void,
  ): () => void {
    const emitter = this._deps!.getEventManager().getEmitter(executionId);
    return emitter.on("FOLLOWUP_QUESTION_REQUESTED", listener);
  }
}