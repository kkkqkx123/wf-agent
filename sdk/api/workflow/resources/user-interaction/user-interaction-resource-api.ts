/**
 * UserInteractionResourceAPI - UserInteractionResourceAPI
 * Provides user interaction related resource management functions
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
} from "../../../shared/validation/validation-strategy.js";

import { now, diffTimestamp } from "@wf-agent/common-utils";
import { CrudResourceAPI } from "../../../shared/resources/generic-resource-api.js";
import type { ExecutionResult } from "../../../shared/types/execution-result.js";
import { success, failure } from "../../../shared/types/execution-result.js";
import type { UserInteractionHandler, UserInteractionRequest } from "@wf-agent/types";
import { ConfigurationError } from "@wf-agent/types";
import type {
  UserInteractionRequestedEvent,
  UserInteractionRespondedEvent,
  UserInteractionProcessedEvent,
  UserInteractionFailedEvent,
} from "@wf-agent/types";
import type { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";

/**
 * User Interaction Configuration
 */
export interface UserInteractionConfig {
  /** layout ID */
  id: string;
  /** Placement Name */
  name: string;
  /** Configuration Description */
  description?: string;
  /** Default timeout in milliseconds */
  defaultTimeout?: number;
  /** metadata */
  metadata?: Record<string, unknown>;
}

/**
 * User Interaction Filter
 */
export interface UserInteractionFilter {
  /** Placement Name */
  name?: string;
  /** Metadata Filtering */
  metadata?: Record<string, unknown>;
}

/**
 * User Interaction Resource Management API
 */
export class UserInteractionResourceAPI extends CrudResourceAPI<
  UserInteractionConfig,
  string,
  UserInteractionFilter
> {
  private dependencies: APIDependencyManager;
  private userInteractionHandler?: UserInteractionHandler;
  private configs: Map<string, UserInteractionConfig> = new Map();

  constructor(dependencies: APIDependencyManager) {
    super();
    this.dependencies = dependencies;
  }

  // ============================================================================
  // GenericResourceAPI abstract method implementation
  // ============================================================================

  protected async getResource(id: string): Promise<UserInteractionConfig | null> {
    return this.configs.get(id) || null;
  }

  protected async getAllResources(): Promise<UserInteractionConfig[]> {
    return Array.from(this.configs.values());
  }

  protected async createResource(config: UserInteractionConfig): Promise<void> {
    this.configs.set(config.id, config);
  }

  protected async updateResource(
    id: string,
    updates: Partial<UserInteractionConfig>,
  ): Promise<void> {
    const existing = this.configs.get(id);
    if (existing) {
      this.configs.set(id, { ...existing, ...updates });
    }
  }

  protected async deleteResource(id: string): Promise<void> {
    this.configs.delete(id);
  }

  protected override applyFilter(
    resources: UserInteractionConfig[],
    filter: UserInteractionFilter,
  ): UserInteractionConfig[] {
    return resources.filter(config => {
      if (filter.name && !config.name.includes(filter.name)) {
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

  protected override async clearResources(): Promise<void> {
    this.configs.clear();
  }

  // ============================================================================
  // User Interaction Processor Management
  // ============================================================================

  /**
   * Registering a User Interaction Handler
   * @param handler User interaction handler
   */
  registerHandler(handler: UserInteractionHandler): void {
    this.userInteractionHandler = handler;
  }

  /**
   * Get the currently registered User Interaction Processor
   * @returns The user interaction processor, or undefined if not registered.
   */
  getHandler(): UserInteractionHandler | undefined {
    return this.userInteractionHandler;
  }

  /**
   * Clear the currently registered User Interaction Processor
   */
  clearHandler(): void {
    this.userInteractionHandler = undefined;
  }

  // ============================================================================
  // User Interaction Processing
  // ============================================================================

  /**
   * Handling User Interaction Requests
   * @param request User interaction request
   * @returns Execution results
   */
  async handleInteraction(request: UserInteractionRequest): Promise<ExecutionResult<unknown>> {
    const startTime = now();

    try {
      if (!this.userInteractionHandler) {
        return failure(
          new ConfigurationError(
            "UserInteractionHandler not registered. Please register a handler before handling interactions.",
            "userInteractionHandler",
            { code: "HANDLER_NOT_REGISTERED" },
          ),
          diffTimestamp(startTime, now()),
        );
      }

      // Creating an Interaction Context
      const context = this.createInteractionContext(request);

      // invocation processor
      const result = await this.userInteractionHandler.handle(request, context);

      return success(result, diffTimestamp(startTime, now()));
    } catch (error) {
      return this.handleError(error, "HANDLE_INTERACTION", startTime);
    }
  }

  /**
   * Creating an Interaction Context
   * @param request User interaction request
   * @returns Interaction context
   */
  private createInteractionContext(
    request: UserInteractionRequest,
  ): import("@wf-agent/types").UserInteractionContext {
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
        // Simplifying the implementation, you should actually get the ThreadContext from the
        return undefined;
      },
      setVariable: async () => {
        // Simplifying the implementation, you should actually update the variables in the ThreadContext
      },
      getVariables: () => {
        // Simplifying the implementation, you should actually get the ThreadContext from the
        return {};
      },
      timeout: request.timeout,
      cancelToken: cancelToken as import("@wf-agent/types").UserInteractionContext["cancelToken"],
    };
  }

  // ============================================================================
  // event subscription
  // ============================================================================

  /**
   * Subscribe to user interaction request events
   * @param listener event listener
   */
  onInteractionRequested(listener: (event: UserInteractionRequestedEvent) => void): void {
    this.dependencies.getEventManager().on("USER_INTERACTION_REQUESTED", listener);
  }

  /**
   * Unsubscribe from user interaction request events
   * @param listener event listener
   */
  offInteractionRequested(listener: (event: UserInteractionRequestedEvent) => void): void {
    this.dependencies.getEventManager().off("USER_INTERACTION_REQUESTED", listener);
  }

  /**
   * Subscribe to user interaction events
   * @param listener event listener
   */
  onInteractionResponded(listener: (event: UserInteractionRespondedEvent) => void): void {
    this.dependencies.getEventManager().on("USER_INTERACTION_RESPONDED", listener);
  }

  /**
   * Unsubscribe from user interaction response events
   * @param listener event listener
   */
  offInteractionResponded(listener: (event: UserInteractionRespondedEvent) => void): void {
    this.dependencies.getEventManager().off("USER_INTERACTION_RESPONDED", listener);
  }

  /**
   * Subscribe to user interaction completion events
   * @param listener event listener
   */
  onInteractionProcessed(listener: (event: UserInteractionProcessedEvent) => void): void {
    this.dependencies.getEventManager().on("USER_INTERACTION_PROCESSED", listener);
  }

  /**
   * Unsubscribe from the user interaction completion event
   * @param listener event listener
   */
  offInteractionProcessed(listener: (event: UserInteractionProcessedEvent) => void): void {
    this.dependencies.getEventManager().off("USER_INTERACTION_PROCESSED", listener);
  }

  /**
   * Subscribe to user interaction failure events
   * @param listener event listener
   */
  onInteractionFailed(listener: (event: UserInteractionFailedEvent) => void): void {
    this.dependencies.getEventManager().on("USER_INTERACTION_FAILED", listener);
  }

  /**
   * Unsubscribe from user interaction failure events
   * @param listener event listener
   */
  offInteractionFailed(listener: (event: UserInteractionFailedEvent) => void): void {
    this.dependencies.getEventManager().off("USER_INTERACTION_FAILED", listener);
  }

  // ============================================================================
  // Validation Methods
  // ============================================================================

  /**
   * Validating User Interaction Configuration
   * @param config Configuration object
   * @returns Validation results
   */
  protected override async validateResource(
    config: UserInteractionConfig,
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

    return {
      valid: errors.length === 0,
      errors,
    };
  }

  /**
   * Verify user interaction with configuration updates
   * @param updates Updated content
   * @returns Validation results
   */
  protected override async validateUpdate(
    updates: Partial<UserInteractionConfig>,
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

    return {
      valid: errors.length === 0,
      errors,
    };
  }

  // ============================================================================
  // Tools and methodologies
  // ============================================================================

  /**
   * Checking if a Processor is Registered
   * @returns whether the processor is registered
   */
  hasHandler(): boolean {
    return this.userInteractionHandler !== undefined;
  }

  /**
   * Getting Configuration Quantity
   * @returns Execution results
   */
  async getConfigCount(): Promise<ExecutionResult<number>> {
    const startTime = now();
    try {
      return success(this.configs.size, diffTimestamp(startTime, now()));
    } catch (error) {
      return this.handleError(error, "GET_CONFIG_COUNT", startTime);
    }
  }
}
