/**
 * UserInteractionResourceAPI - UserInteractionResourceAPI
 * Provides user interaction related resource management functions
 *
 * Extends BaseUserInteractionResourceAPI with workflow-specific event subscription capabilities.
 *
 * Responsibilities:
 * - Manage the registration and acquisition of user interaction processors
 * - Provide user interaction event subscription functionality
 * - Encapsulate user interaction processing logic
 *
 * Design Principles:
 * - Follow the GenericResourceAPI unified interface pattern.
 * - Provide clear event subscription interface
 * - Support dynamic registration and replacement of processors
 */

import {
  validateRequiredFields,
  validateStringLength,
  validatePositiveNumber,
} from "../../shared/validation/validation-strategy.js";

import { now, diffTimestamp } from "@wf-agent/common-utils";
import {
  BaseUserInteractionResourceAPI,
  type BaseUserInteractionConfig,
  type BaseUserInteractionFilter,
  type UserInteractionEventRecord,
} from "../../shared/resources/user-interaction-base.js";
import type { ExecutionResult } from "../../shared/types/execution-result.js";
import { success, failure } from "../../shared/types/execution-result.js";
import type {
  UserInteractionRequest,
  UserInteractionContext,
} from "@wf-agent/types";
import { ConfigurationError, SDKError } from "@wf-agent/types";
import type { ToolApprovalRequestedEvent, FollowupQuestionRequestedEvent } from "@wf-agent/types";
import type { APIDependencyManager } from "../../shared/core/sdk-dependencies.js";

/**
 * User Interaction Configuration
 */
export interface UserInteractionConfig extends BaseUserInteractionConfig {
  /** Configuration Description */
  description?: string;
  /** Default timeout in milliseconds */
  defaultTimeout?: number;
}

/**
 * User Interaction Filter
 */
export interface UserInteractionFilter extends BaseUserInteractionFilter {
  /** Placement Name */
  name?: string;
  /** Metadata Filtering */
  metadata?: Record<string, unknown>;
}

/**
 * User Interaction Resource Management API
 */
export class UserInteractionResourceAPI extends BaseUserInteractionResourceAPI<
  UserInteractionConfig,
  UserInteractionFilter
> {
  private dependencies: APIDependencyManager;

  constructor(dependencies: APIDependencyManager) {
    super();
    this.dependencies = dependencies;
  }

  // ============================================================================
  // Implement BaseUserInteractionResourceAPI abstract methods
  // ============================================================================

  protected override applyFilter(
    resources: UserInteractionConfig[],
    filter: UserInteractionFilter,
  ): UserInteractionConfig[] {
    return resources.filter(config => {
      if (filter.name && !config.name.includes(filter.name)) {
        return false;
      }
      if (filter.type && config.type !== filter.type) {
        return false;
      }
      if (filter.enabled !== undefined && config.enabled !== filter.enabled) {
        return false;
      }
      if (filter.metadata) {
        for (const [key, value] of Object.entries(filter.metadata)) {
          if (config.metadata?.[key] !== value) {
            return false;
          }
        }
      }
      return true;
    });
  }

  /**
   * Validating User Interaction Configuration
   * @param config Configuration object
   * @param _context Validation context
   */
  protected override async validateResource(
    config: UserInteractionConfig,
    _context?: unknown,
  ): Promise<{ valid: boolean; errors: string[] }> {
    const errors: string[] = [];

    // Validating Required Fields with Simplified Validation Tool
    const requiredResult = validateRequiredFields(config, ["id", "name"], "config");
    if (requiredResult.isErr()) {
      errors.push(...requiredResult.unwrapOrElse(err => err.map(error => error.message)));
    }

    // Authentication Timeout
    if (config.defaultTimeout !== undefined) {
      const timeoutResult = validatePositiveNumber(config.defaultTimeout, "Default timeout");
      if (timeoutResult.isErr()) {
        errors.push(...timeoutResult.unwrapOrElse(err => err.map(error => error.message)));
      }
    }

    // Verify ID length
    if (config.id) {
      const idResult = validateStringLength(config.id, "layout ID", 1, 100);
      if (idResult.isErr()) {
        errors.push(...idResult.unwrapOrElse(err => err.map(error => error.message)));
      }
    }

    // Verify name length
    if (config.name) {
      const nameResult = validateStringLength(config.name, "Placement Name", 1, 200);
      if (nameResult.isErr()) {
        errors.push(...nameResult.unwrapOrElse(err => err.map(error => error.message)));
      }
    }

    return { valid: errors.length === 0, errors };
  }

  /**
   * Verify user interaction with configuration updates
   * @param updates Updated content
   * @param _context Validation context
   */
  protected override async validateUpdate(
    updates: Partial<UserInteractionConfig>,
    _context?: unknown,
  ): Promise<{ valid: boolean; errors: string[] }> {
    const errors: string[] = [];

    // Validation name (if provided)
    if (updates.name !== undefined) {
      const nameResult = validateStringLength(updates.name, "Placement Name", 1, 200);
      if (nameResult.isErr()) {
        errors.push(...nameResult.unwrapOrElse(err => err.map(error => error.message)));
      }
    }

    // Validation timeout time (if provided)
    if (updates.defaultTimeout !== undefined) {
      const timeoutResult = validatePositiveNumber(updates.defaultTimeout, "Default timeout");
      if (timeoutResult.isErr()) {
        errors.push(...timeoutResult.unwrapOrElse(err => err.map(error => error.message)));
      }
    }

    return { valid: errors.length === 0, errors };
  }

  // ============================================================================
  // Override: Interaction Context with Workflow-specific capabilities
  // ============================================================================

  /**
   * Creating an Interaction Context
   * @param request User interaction request
   * @returns Interaction context
   */
  protected override createInteractionContext(request: UserInteractionRequest): UserInteractionContext {
    const cancelToken: { cancelled: boolean; cancel: () => void } = {
      cancelled: false,
      cancel: () => {
        cancelToken.cancelled = true;
      },
    };

    return {
      executionId: (request.metadata?.["executionId"] as string) || "",
      workflowId: (request.metadata?.["workflowId"] as string) || "",
      nodeId: (request.metadata?.["nodeId"] as string) || "",
      getVariable: () => {
        // Simplifying the implementation, you should actually get the WorkflowExecutionContext from the
        return undefined;
      },
      setVariable: async () => {
        // Simplifying the implementation, you should actually update the variables in the WorkflowExecutionContext
      },
      getVariables: () => {
        // Simplifying the implementation, you should actually get the WorkflowExecutionContext from the
        return {};
      },
      timeout: request.timeout,
      cancelToken: cancelToken as UserInteractionContext["cancelToken"],
    };
  }

  /**
   * Handle errors during interaction processing
   * @param error The error that occurred
   * @param _operationType Type of operation for error context
   * @param startTime Start time for duration calculation
   * @returns Failure execution result
   */
  private handleWorkflowError(error: unknown, _operationType: string, startTime: number): ExecutionResult<unknown> {
    return failure(
      error instanceof SDKError ? error : new ConfigurationError(String(error), "workflow", { code: "WORKFLOW_ERROR" }),
      diffTimestamp(startTime, now()),
    );
  }

  // ============================================================================
  // Specialized Event Subscriptions
  // ============================================================================

  /**
   * Subscribe to tool approval request events
   * @param executionId Execution ID (required)
   * @param listener event listener
   * @returns Unsubscribe function
   */
  onToolApprovalRequested(
    executionId: string,
    listener: (event: ToolApprovalRequestedEvent) => void,
  ): () => void {
    const emitter = this.dependencies.getEventManager().getEmitter(executionId);
    return emitter.on("TOOL_APPROVAL_REQUESTED", listener);
  }

  /**
   * Subscribe to follow-up question request events
   * @param executionId Execution ID (required)
   * @param listener event listener
   * @returns Unsubscribe function
   */
  onFollowupQuestionRequested(
    executionId: string,
    listener: (event: FollowupQuestionRequestedEvent) => void,
  ): () => void {
    const emitter = this.dependencies.getEventManager().getEmitter(executionId);
    return emitter.on("FOLLOWUP_QUESTION_REQUESTED", listener);
  }

  // ============================================================================
  // Tools and methodologies
  // ============================================================================

  /**
   * Getting Configuration Quantity
   * @returns Execution results
   */
  async getConfigCount(): Promise<ExecutionResult<number>> {
    const startTime = now();
    try {
      return success(this.configs.size, diffTimestamp(startTime, now()));
    } catch (error) {
      return this.handleWorkflowError(error, "GET_CONFIG_COUNT", startTime) as ExecutionResult<number>;
    }
  }

  // ============================================================================
  // Workflow-specific: Interaction Handling (with event recording)
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
          timeout: config.defaultTimeout ?? 30000,
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
          timeout: config.defaultTimeout ?? 30000,
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
  // Workflow-specific: Event History
  // ============================================================================

  /**
   * Get configuration interaction history
   * @param configId Configuration ID
   * @returns Array of interaction event records for the specified config
   */
  async getConfigurationInteractionHistory(configId: string): Promise<UserInteractionEventRecord[]> {
    return (await this.getInteractionEventHistory()).filter(event => event.configId === configId);
  }
}