/**
 * AgentUserInteractionResourceAPI - Agent User Interaction Resource Management API
 * Manages user interaction configurations and handlers for agent executions
 */

import { validateRequiredFields } from "../../shared/validation/validation-strategy.js";
import { now } from "@wf-agent/common-utils";
import { SimplifiedCrudResourceAPI } from "../../shared/resources/generic-resource-api.js";
import type {
  UserInteractionHandler,
} from "@wf-agent/types";
import { ConfigurationError } from "@wf-agent/types";

/**
 * Agent User Interaction Configuration
 */
export interface AgentUserInteractionConfig {
  /** Configuration ID */
  id: string;
  /** Configuration Name */
  name: string;
  /** Configuration Description */
  description?: string;
  /** Default timeout in milliseconds */
  defaultTimeout?: number;
  /** Interaction Type (approval, input, confirmation, etc.) */
  interactionType?: "approval" | "input" | "confirmation" | "selection" | "custom";
  /** metadata */
  metadata?: Record<string, unknown>;
}

/**
 * Agent User Interaction Filter
 */
export interface AgentUserInteractionFilter {
  /** Configuration Name */
  name?: string;
  /** Interaction Type */
  interactionType?: string;
  /** Metadata Filtering */
  metadata?: Record<string, unknown>;
}

/**
 * Agent User Interaction Event Record
 */
export interface AgentUserInteractionEventRecord {
  /** Event ID */
  id: string;
  /** Configuration ID that triggered this event */
  configId: string;
  /** Execution ID */
  executionId: string;
  /** Event Type */
  eventType: "requested" | "approved" | "rejected" | "timeout" | "cancelled";
  /** Request Context */
  context?: Record<string, unknown>;
  /** User Response */
  response?: unknown;
  /** Event Timestamp */
  timestamp: number;
  /** Response Timestamp */
  responseTimestamp?: number;
  /** Response Time (ms) */
  responseTime?: number | null;
}

/**
 * Agent User Interaction Resource Management API
 */
export class AgentUserInteractionResourceAPI extends SimplifiedCrudResourceAPI<
  AgentUserInteractionConfig,
  string,
  AgentUserInteractionFilter
> {
  private userInteractionHandler?: UserInteractionHandler;
  private configs: Map<string, AgentUserInteractionConfig> = new Map();
  private eventHistory: AgentUserInteractionEventRecord[] = [];
  private eventIdCounter: number = 0;

  constructor() {
    super();
  }

  // ============================================================================
  // GenericResourceAPI abstract method implementation
  // ============================================================================

  protected async getResource(id: string): Promise<AgentUserInteractionConfig | null> {
    return this.configs.get(id) || null;
  }

  protected async getAllResources(): Promise<AgentUserInteractionConfig[]> {
    return Array.from(this.configs.values());
  }

  protected async createResource(config: AgentUserInteractionConfig): Promise<void> {
    // Validate required fields
    validateRequiredFields(config, ["id", "name"], "config");
    // Note: validateStringLength returns a Result type, we perform basic validation instead
    if (!config.id || !config.name) {
      throw new Error("Configuration must have id and name");
    }
    if (config.name.length < 1 || config.name.length > 255) {
      throw new Error("Configuration name must be between 1 and 255 characters");
    }

    this.configs.set(config.id, config);
  }

  protected async updateResource(
    id: string,
    updates: Partial<AgentUserInteractionConfig>,
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
    resources: AgentUserInteractionConfig[],
    filter: AgentUserInteractionFilter,
  ): AgentUserInteractionConfig[] {
    return resources.filter(config => {
      if (filter.name && !config.name.includes(filter.name)) {
        return false;
      }

      if (filter.interactionType && config.interactionType !== filter.interactionType) {
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

  // ============================================================================
  // Agent-specific user interaction methods
  // ============================================================================

  /**
   * Register a user interaction handler
   * @param handler The user interaction handler
   */
  async registerUserInteractionHandler(handler: UserInteractionHandler): Promise<void> {
    this.userInteractionHandler = handler;
  }

  /**
   * Get the registered user interaction handler
   * @returns The handler, or undefined if not registered
   */
  async getUserInteractionHandler(): Promise<UserInteractionHandler | undefined> {
    return this.userInteractionHandler;
  }

  /**
   * Handle a user interaction request
   * @param configId Configuration ID
   * @param executionId Execution ID
   * @param context Interaction context
   * @returns User response
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
      // If a handler is registered, call it
      if (this.userInteractionHandler) {
        // For now, we do not directly call the handler with UserInteractionRequest
        // as it requires specific fields (interactionId, operationType, prompt, timeout)
        // Applications should implement their own UserInteractionHandler independently
      }

      // Update event with response (simplified - no actual handler call)
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

  /**
   * Get user interaction event history
   * @param executionId Optional execution ID to filter events
   * @returns Array of interaction events
   */
  async getInteractionEventHistory(executionId?: string): Promise<AgentUserInteractionEventRecord[]> {
    if (executionId) {
      return this.eventHistory.filter(event => event.executionId === executionId);
    }
    return [...this.eventHistory];
  }

  /**
   * Get interaction statistics
   * @returns Statistics about user interactions
   */
  async getInteractionStatistics(): Promise<{
    totalInteractions: number;
    byType: Record<string, number>;
    byStatus: Record<string, number>;
    averageResponseTime: number;
    totalResponseTime: number;
  }> {
    const stats = {
      totalInteractions: this.eventHistory.length,
      byType: {} as Record<string, number>,
      byStatus: {
        requested: 0,
        approved: 0,
        rejected: 0,
        timeout: 0,
        cancelled: 0,
      },
      averageResponseTime: 0,
      totalResponseTime: 0,
    };

    for (const event of this.eventHistory) {
      // Count by interaction type
      const config = this.configs.get(event.configId);
      const type = config?.interactionType || "unknown";
      stats.byType[type] = (stats.byType[type] || 0) + 1;

      // Count by status
      stats.byStatus[event.eventType]++;

      // Calculate response time
      if (event.responseTime !== undefined && event.responseTime !== null) {
        stats.totalResponseTime += event.responseTime;
      }
    }

    if (this.eventHistory.length > 0) {
      stats.averageResponseTime = stats.totalResponseTime / this.eventHistory.length;
    }

    return stats;
  }

  /**
   * Get interaction events for a specific configuration
   * @param configId Configuration ID
   * @returns Array of events for this configuration
   */
  async getConfigurationInteractionHistory(configId: string): Promise<AgentUserInteractionEventRecord[]> {
    return this.eventHistory.filter(event => event.configId === configId);
  }

  /**
   * Clear interaction event history
   * @param olderThanTimestamp Optional: only clear events older than this timestamp
   */
  async clearInteractionHistory(olderThanTimestamp?: number): Promise<void> {
    if (olderThanTimestamp) {
      this.eventHistory = this.eventHistory.filter(event => event.timestamp > olderThanTimestamp);
    } else {
      this.eventHistory = [];
    }
  }

  /**
   * Export interaction history
   * @param executionId Optional: filter by execution ID
   * @returns JSON string
   */
  async exportInteractionHistory(executionId?: string): Promise<string> {
    const history = await this.getInteractionEventHistory(executionId);
    return JSON.stringify(history, null, 2);
  }

  // ============================================================================
  // Helper methods
  // ============================================================================

  /**
   * Generate unique event ID
   */
  private generateEventId(): string {
    return `event_${++this.eventIdCounter}_${now()}`;
  }
}
