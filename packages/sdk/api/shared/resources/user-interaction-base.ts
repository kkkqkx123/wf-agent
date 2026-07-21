/**
 * BaseUserInteractionResourceAPI - Shared base class for user interaction resource management
 *
 * Provides a unified base implementation for user interaction CRUD and handler operations.
 * Both Agent and Workflow versions extend this class, providing only entity-specific
 * type annotations and method naming.
 *
 * Follows the same pattern as BaseMessageResourceAPI for shared base class design.
 *
 * Entity-specific subclasses must specify:
 * - TConfig: The user interaction config type (AgentUserInteractionConfig or UserInteractionConfig)
 * - TFilter: The filter type (AgentUserInteractionFilter or UserInteractionFilter)
 */

import { SimplifiedCrudResourceAPI } from "./generic-resource-api.js";
import type {
  UserInteractionHandler,
  UserInteractionRequest,
  UserInteractionContext,
} from "@wf-agent/types";
import type { ExecutionResult } from "../types/execution-result.js";
import { success, failure } from "../types/execution-result.js";
import { ConfigurationError, SDKError } from "@wf-agent/types";
import { now, diffTimestamp } from "@wf-agent/common-utils";

/**
 * Base user interaction config - entity-specific configs extend this
 */
export interface BaseUserInteractionConfig {
  /** Configuration ID */
  id: string;
  /** Configuration name */
  name: string;
  /** Type of interaction */
  type: string;
  /** Whether this configuration is enabled */
  enabled: boolean;
  /** Additional metadata */
  metadata?: Record<string, unknown>;
}

/**
 * Base user interaction filter - entity-specific filters extend this
 */
export interface BaseUserInteractionFilter {
  /** Filter by type */
  type?: string;
  /** Filter by enabled state */
  enabled?: boolean;
}

/**
 * BaseUserInteractionResourceAPI - Shared base class for user interaction resource management
 *
 * Handles common user interaction operations:
 * - Handler registration, retrieval, and clearing
 * - Interaction handling with structured request/response
 * - CRUD operations for interaction configs
 */
export abstract class BaseUserInteractionResourceAPI<
  TConfig extends BaseUserInteractionConfig = BaseUserInteractionConfig,
  TFilter extends BaseUserInteractionFilter = BaseUserInteractionFilter,
> extends SimplifiedCrudResourceAPI<TConfig, string, TFilter> {
  protected userInteractionHandler?: UserInteractionHandler;
  protected configs: Map<string, TConfig> = new Map();

  // ============================================================================
  // GenericResourceAPI abstract method implementation
  // ============================================================================

  protected async getResource(id: string): Promise<TConfig | null> {
    return this.configs.get(id) || null;
  }

  protected async getAllResources(): Promise<TConfig[]> {
    return Array.from(this.configs.values());
  }

  protected async createResource(config: TConfig): Promise<void> {
    this.configs.set(config.id, config);
  }

  protected async updateResource(id: string, updates: Partial<TConfig>): Promise<void> {
    const existing = this.configs.get(id);
    if (!existing) {
      throw new Error(`User interaction config not found: ${id}`);
    }
    await this.validateUpdate(updates);
    this.configs.set(id, { ...existing, ...updates });
  }

  protected async deleteResource(id: string): Promise<void> {
    this.configs.delete(id);
  }

  protected override applyFilter(
    configs: TConfig[],
    filter: TFilter,
  ): TConfig[] {
    return configs.filter(config => {
      if (filter.type && config.type !== filter.type) {
        return false;
      }
      if (filter.enabled !== undefined && config.enabled !== filter.enabled) {
        return false;
      }
      return true;
    });
  }

  protected override async clearResources(): Promise<void> {
    this.configs.clear();
  }

  // ============================================================================
  // Common Handler Management
  // ============================================================================

  /**
   * Register a user interaction handler
   * @param handler The user interaction handler to register
   */
  registerHandler(handler: UserInteractionHandler): void {
    this.userInteractionHandler = handler;
  }

  /**
   * Get the currently registered user interaction handler
   * @returns The handler, or undefined if not registered
   */
  getHandler(): UserInteractionHandler | undefined {
    return this.userInteractionHandler;
  }

  /**
   * Clear the registered user interaction handler
   */
  clearHandler(): void {
    this.userInteractionHandler = undefined;
  }

  /**
   * Check if a user interaction handler is registered
   * @returns true if a handler is registered
   */
  hasHandler(): boolean {
    return this.userInteractionHandler !== undefined;
  }

  // ============================================================================
  // Common Interaction Handling
  // ============================================================================

  /**
   * Handle a user interaction request
   * Delegates to the registered handler if available.
   * @param request User interaction request
   * @returns Execution result with the handler's response
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

      // Create interaction context
      const context = this.createInteractionContext(request);

      // Invoke the handler
      const result = await this.userInteractionHandler.handle(request, context);

      return success(result, diffTimestamp(startTime, now()));
    } catch (error) {
      return this.handleInteractionError(error, startTime);
    }
  }

  /**
   * Create an interaction context from a request.
   * Subclasses can override this to provide domain-specific context.
   * @param request User interaction request
   * @returns Interaction context
   */
  protected createInteractionContext(request: UserInteractionRequest): UserInteractionContext {
    const cancelToken: { cancelled: boolean; cancel: () => void } = {
      cancelled: false,
      cancel: () => {
        cancelToken.cancelled = true;
      },
    };

    return {
      executionId: (request.metadata?.["executionId"] as string) ?? "",
      workflowId: (request.metadata?.["workflowId"] as string) ?? "",
      nodeId: (request.metadata?.["nodeId"] as string) ?? "",
      getVariable: (_variableName: string) => undefined,
      setVariable: async (_variableName: string, _value: unknown) => {},
      getVariables: () => ({}),
      timeout: request.timeout,
      cancelToken: cancelToken as UserInteractionContext["cancelToken"],
    };
  }

  /**
   * Handle errors during interaction processing
   * @param error The error that occurred
   * @param startTime Start time for duration calculation
   * @returns Failure execution result
   */
  protected handleInteractionError<T>(error: unknown, startTime: number): ExecutionResult<T> {
    return failure(
      error instanceof SDKError ? error : new ConfigurationError(String(error), "interaction", { code: "INTERACTION_ERROR" }),
      diffTimestamp(startTime, now()),
    );
  }
}