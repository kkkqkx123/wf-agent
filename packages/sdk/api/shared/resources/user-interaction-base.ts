/**
 * BaseUserInteractionResourceAPI - Shared base class for user interaction resource management
 *
 * Provides a unified base implementation for user interaction CRUD and handler operations.
 * Both Agent and Workflow versions extend this class, providing only entity-specific
 * type annotations and method naming.
 *
 * Features:
 * - Handler registration and interaction handling
 * - Optional event recording (enabled via enableEventRecording)
 * - Optional event subscriptions (via setEventManager)
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
import type { EventRegistry } from "../../../shared/registry/event-registry.js";

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
 * User interaction event record for event history tracking
 */
export interface UserInteractionEventRecord {
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
  // Optional Event Recording
  // ============================================================================

  /** Event history for optional event recording */
  protected eventHistory: UserInteractionEventRecord[] = [];
  /** Whether event recording is enabled */
  protected eventRecordingEnabled: boolean = false;

  /**
   * Enable or disable event recording
   * @param enabled Whether to enable event recording
   */
  setEventRecordingEnabled(enabled: boolean): void {
    this.eventRecordingEnabled = enabled;
  }

  /**
   * Check if event recording is enabled
   */
  isEventRecordingEnabled(): boolean {
    return this.eventRecordingEnabled;
  }

  // ============================================================================
  // Optional Event Subscriptions
  // ============================================================================

  /** Event manager for optional event subscriptions */
  protected eventManager?: EventRegistry;

  /**
   * Set the event manager for event subscriptions
   * @param em Event registry instance
   */
  setEventManager(em: EventRegistry): void {
    this.eventManager = em;
  }

  /**
   * Get the current event manager
   */
  getEventManager(): EventRegistry | undefined {
    return this.eventManager;
  }

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

  // ============================================================================
  // Shared Event Recording Methods
  // ============================================================================

  /**
   * Record an interaction event
   * Only records if event recording is enabled.
   * @param configId Configuration ID
   * @param executionId Execution ID
   * @param eventType Event type
   * @param context Event context
   * @returns The recorded event record
   */
  protected recordInteractionEvent(
    configId: string,
    executionId: string,
    eventType: string,
    context: Record<string, unknown>,
  ): UserInteractionEventRecord {
    const event: UserInteractionEventRecord = {
      id: `${configId}:${executionId}:${now()}:${Math.random().toString(36).slice(2, 8)}`,
      configId,
      executionId,
      eventType,
      context,
      timestamp: now(),
      responseTime: null,
    };

    if (this.eventRecordingEnabled) {
      this.eventHistory.push(event);
    }

    return event;
  }

  /**
   * Update a recorded event with a response
   * @param event The event to update
   * @param response The response value
   */
  protected completeInteractionEvent(event: UserInteractionEventRecord, response: unknown): void {
    const responseTimestamp = now();
    event.response = response;
    event.responseTimestamp = responseTimestamp;
    event.responseTime = responseTimestamp - event.timestamp;
  }

  /**
   * Get interaction event history
   * @param executionId Optional execution ID to filter by
   * @returns Array of interaction event records
   */
  async getInteractionEventHistory(executionId?: string): Promise<UserInteractionEventRecord[]> {
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
}