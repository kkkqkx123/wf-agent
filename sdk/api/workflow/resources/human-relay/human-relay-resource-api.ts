/**
 * HumanRelayResourceAPI - Human Relay Resource Management API
 * Provides Human Relay related resource management functionality
 *
 * Responsibilities:
 * - Manages registration and retrieval of Human Relay handlers
 * - Provides Human Relay event subscription functionality
 * - Encapsulates Human Relay processing logic
 *
 * Design Principles:
 * - Follows the unified interface pattern of GenericResourceAPI
 * - Provides clear event subscription interface
 * - Supports dynamic registration and replacement of handlers
 */

import {
  validateRequiredFields,
  validateStringLength,
  validatePositiveNumber,
  validateBoolean,
} from "../../../shared/validation/validation-strategy.js";

import { now, diffTimestamp } from "@wf-agent/common-utils";
import { CrudResourceAPI } from "../../../shared/resources/generic-resource-api.js";
import type { ExecutionResult } from "../../../shared/types/execution-result.js";
import { success, failure } from "../../../shared/types/execution-result.js";
import type { HumanRelayHandler, HumanRelayRequest, HumanRelayResponse } from "@wf-agent/types";
import { ConfigurationError, NotFoundError } from "@wf-agent/types";
import type {
  HumanRelayRequestedEvent,
  HumanRelayRespondedEvent,
  HumanRelayProcessedEvent,
  HumanRelayFailedEvent,
} from "@wf-agent/types";
import type { APIDependencyManager } from "../../../shared/core/sdk-dependencies.js";

/**
 * Human Relay Configuration
 */
export interface HumanRelayConfig {
  /** layout ID */
  id: string;
  /** Placement Name */
  name: string;
  /** Configuration Description */
  description?: string;
  /** Default timeout in milliseconds */
  defaultTimeout?: number;
  /** Enable or disable */
  enabled?: boolean;
  /** metadata */
  metadata?: Record<string, unknown>;
}

/**
 * Human Relay Filter
 */
export interface HumanRelayFilter {
  /** Placement Name */
  name?: string;
  /** Enable or disable */
  enabled?: boolean;
  /** Metadata Filtering */
  metadata?: Record<string, unknown>;
}

/**
 * Human Relay Resource Management API
 */
export class HumanRelayResourceAPI extends CrudResourceAPI<
  HumanRelayConfig,
  string,
  HumanRelayFilter
> {
  private dependencies: APIDependencyManager;
  private humanRelayHandler?: HumanRelayHandler;
  private configs: Map<string, HumanRelayConfig> = new Map();

  constructor(dependencies: APIDependencyManager) {
    super();
    this.dependencies = dependencies;
  }

  // ============================================================================
  // GenericResourceAPI abstract method implementation
  // ============================================================================

  protected async getResource(id: string): Promise<HumanRelayConfig | null> {
    return this.configs.get(id) || null;
  }

  protected async getAllResources(): Promise<HumanRelayConfig[]> {
    return Array.from(this.configs.values());
  }

  protected async createResource(config: HumanRelayConfig): Promise<void> {
    this.configs.set(config.id, config);
  }

  protected async updateResource(id: string, updates: Partial<HumanRelayConfig>): Promise<void> {
    const existing = this.configs.get(id);
    if (existing) {
      this.configs.set(id, { ...existing, ...updates });
    }
  }

  protected async deleteResource(id: string): Promise<void> {
    this.configs.delete(id);
  }

  protected override applyFilter(
    resources: HumanRelayConfig[],
    filter: HumanRelayFilter,
  ): HumanRelayConfig[] {
    return resources.filter(config => {
      if (filter.name && !config.name.includes(filter.name)) {
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

  protected override async clearResources(): Promise<void> {
    this.configs.clear();
  }

  // ============================================================================
  // Human Relay Processor Management
  // ============================================================================

  /**
   * Registering the Human Relay handler
   * Also sync to ClientFactory via LLMWrapper
   * @param handler Human Relay handler
   */
  registerHandler(handler: HumanRelayHandler): void {
    this.humanRelayHandler = handler;

    // Sync to LLMWrapper's ClientFactory
    const llmWrapper = this.dependencies.getLLMWrapper?.();
    if (llmWrapper) {
      llmWrapper.setHumanRelayHandler(handler);
    }
  }

  /**
   * Get the currently registered Human Relay handler
   * @returns Human Relay processor, or undefined if not registered
   */
  getHandler(): HumanRelayHandler | undefined {
    return this.humanRelayHandler;
  }

  /**
   * Clear the currently registered Human Relay processor
   */
  clearHandler(): void {
    this.humanRelayHandler = undefined;
  }

  // ============================================================================
  // Human Relay processing
  // ============================================================================

  /**
   * Handle a Human Relay request
   * @param request The Human Relay request
   * @returns The execution result
   */
  async handleRequest(request: HumanRelayRequest): Promise<ExecutionResult<HumanRelayResponse>> {
    const startTime = now();

    try {
      if (!this.humanRelayHandler) {
        return failure(
          new ConfigurationError(
            "HumanRelayHandler not registered. Please register a handler before handling relay requests.",
            "humanRelayHandler",
            { code: "HANDLER_NOT_REGISTERED" },
          ),
          diffTimestamp(startTime, now()),
        );
      }

      // Creating a Relay Context
      const context = this.createRelayContext(request);

      // invocation processor
      const response = await this.humanRelayHandler.handle(request, context);

      return success(response, diffTimestamp(startTime, now()));
    } catch (error) {
      return this.handleError(error, "HANDLE_RELAY_REQUEST", startTime);
    }
  }

  /**
   * Create a Relay context
   * @param request: A Human Relay request
   * @returns: The Relay context
   */
  private createRelayContext(
    request: HumanRelayRequest,
  ): import("@wf-agent/types").HumanRelayContext {
    const cancelToken: { cancelled: boolean; cancel: () => void } = {
      cancelled: false,
      cancel: () => {
        cancelToken.cancelled = true;
      },
    };

    return {
      threadId: (request.metadata?.["threadId"] as string) || "",
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
      cancelToken: cancelToken as import("@wf-agent/types").HumanRelayContext["cancelToken"],
    };
  }

  // ============================================================================
  // event subscription
  // ============================================================================

  /**
   * Subscribe to Human Relay request events
   * @param listener Event listener
   */
  onRelayRequested(listener: (event: HumanRelayRequestedEvent) => void): void {
    this.dependencies.getEventManager().on("HUMAN_RELAY_REQUESTED", listener);
  }

  /**
   * Unsubscribe from Human Relay request events
   * @param listener Event listener
   */
  offRelayRequested(listener: (event: HumanRelayRequestedEvent) => void): void {
    this.dependencies.getEventManager().off("HUMAN_RELAY_REQUESTED", listener);
  }

  /**
   * Subscribe to Human Relay response events
   * @param listener event listener
   */
  onRelayResponded(listener: (event: HumanRelayRespondedEvent) => void): void {
    this.dependencies.getEventManager().on("HUMAN_RELAY_RESPONDED", listener);
  }

  /**
   * Unsubscribe from Human Relay response events
   * @param listener Event listener
   */
  offRelayResponded(listener: (event: HumanRelayRespondedEvent) => void): void {
    this.dependencies.getEventManager().off("HUMAN_RELAY_RESPONDED", listener);
  }

  /**
   * Subscribe to the Human Relay processing completion event
   * @param listener event listener
   */
  onRelayProcessed(listener: (event: HumanRelayProcessedEvent) => void): void {
    this.dependencies.getEventManager().on("HUMAN_RELAY_PROCESSED", listener);
  }

  /**
   * Unsubscribe from the Human Relay processing completion event
   * @param listener Event listener
   */
  offRelayProcessed(listener: (event: HumanRelayProcessedEvent) => void): void {
    this.dependencies.getEventManager().off("HUMAN_RELAY_PROCESSED", listener);
  }

  /**
   * Subscribe to Human Relay failure events
   * @param listener Event listener
   */
  onRelayFailed(listener: (event: HumanRelayFailedEvent) => void): void {
    this.dependencies.getEventManager().on("HUMAN_RELAY_FAILED", listener);
  }

  /**
   * Unsubscribe from Human Relay failure event
   * @param listener Event listener
   */
  offRelayFailed(listener: (event: HumanRelayFailedEvent) => void): void {
    this.dependencies.getEventManager().off("HUMAN_RELAY_FAILED", listener);
  }

  // ============================================================================
  // Validation Methods
  // ============================================================================

  /**
   * Validating the Human Relay Configuration
   * @param config Configuration object
   * @returns Validation results
   */
  protected override async validateResource(
    config: HumanRelayConfig,
  ): Promise<{ valid: boolean; errors: string[] }> {
    const errors: string[] = [];

    // Validating Required Fields with Simplified Validation Tool
    const requiredResult = validateRequiredFields(config, ["id", "name"], "config");
    if (requiredResult.isErr()) {
      errors.push(...requiredResult.error.map(error => error.message));
    }

    // Verify ID length
    if (config.id) {
      const idResult = validateStringLength(config.id, "layout ID", 1, 100);
      if (idResult.isErr()) {
        errors.push(...idResult.error.map(error => error.message));
      }
    }

    // Verify name length
    if (config.name) {
      const nameResult = validateStringLength(config.name, "Placement Name", 1, 200);
      if (nameResult.isErr()) {
        errors.push(...nameResult.error.map(error => error.message));
      }
    }

    // Validation timeout time (if provided)
    if (config.defaultTimeout !== undefined) {
      const timeoutResult = validatePositiveNumber(config.defaultTimeout, "Default timeout");
      if (timeoutResult.isErr()) {
        errors.push(...timeoutResult.error.map(error => error.message));
      }
    }

    // Validate the enabled field (if provided)
    if (config.enabled !== undefined) {
      const enabledResult = validateBoolean(config.enabled, "enabled");
      if (enabledResult.isErr()) {
        errors.push(...enabledResult.error.map(error => error.message));
      }
    }

    return {
      valid: errors.length === 0,
      errors,
    };
  }

  /**
   * Validating Human Relay Configuration Updates
   * @param updates Updated content
   * @returns Validate results
   */
  protected override async validateUpdate(
    updates: Partial<HumanRelayConfig>,
  ): Promise<{ valid: boolean; errors: string[] }> {
    const errors: string[] = [];

    // Validation name (if provided)
    if (updates.name !== undefined) {
      const nameResult = validateStringLength(updates.name, "Placement Name", 1, 200);
      if (nameResult.isErr()) {
        errors.push(...nameResult.error.map(error => error.message));
      }
    }

    // Validation timeout time (if provided)
    if (updates.defaultTimeout !== undefined) {
      const timeoutResult = validatePositiveNumber(updates.defaultTimeout, "Default timeout");
      if (timeoutResult.isErr()) {
        errors.push(...timeoutResult.error.map(error => error.message));
      }
    }

    // Validate the enabled field (if provided)
    if (updates.enabled !== undefined) {
      const enabledResult = validateBoolean(updates.enabled, "enabled");
      if (enabledResult.isErr()) {
        errors.push(...enabledResult.error.map(error => error.message));
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
    return this.humanRelayHandler !== undefined;
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

  /**
   * Enable or disable configuration
   * @param id Configuration ID
   * @param enabled Enable or disable configuration
   * @returns Execution results
   */
  async setConfigEnabled(id: string, enabled: boolean): Promise<ExecutionResult<void>> {
    const startTime = now();
    try {
      const config = this.configs.get(id);
      if (!config) {
        return failure(
          new NotFoundError(`HumanRelayConfig not found: ${id}`, "HumanRelayConfig", id, {
            code: "CONFIG_NOT_FOUND",
          }),
          diffTimestamp(startTime, now()),
        );
      }

      config.enabled = enabled;
      this.configs.set(id, config);

      return success(undefined, diffTimestamp(startTime, now()));
    } catch (error) {
      return this.handleError(error, "SET_CONFIG_ENABLED", startTime);
    }
  }
}
