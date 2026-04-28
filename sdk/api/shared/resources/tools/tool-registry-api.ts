/**
 * ToolRegistryAPI - Tool Resource Management API
 * Encapsulates ToolRegistry, providing functionality for tool registration and querying.
 */

import {
  validateRequiredFields,
  validateStringLength,
  validateObject,
} from "../../validation/validation-strategy.js";

import type { Tool } from "@wf-agent/types";
import { ToolType } from "@wf-agent/types";
import { ToolNotFoundError, NotFoundError } from "@wf-agent/types";
import { CrudResourceAPI } from "../generic-resource-api.js";
import type { APIDependencyManager } from "../../core/sdk-dependencies.js";
import type { Timestamp } from "@wf-agent/types";

/**
 * Tool Filter
 */
export interface ToolFilter {
  /** List of tool IDs */
  ids?: string[];
  /** Tool Name (supports fuzzy search) */
  name?: string;
  /** Tool Type */
  type?: ToolType;
  /** Classification */
  category?: string;
  /** Tag array */
  tags?: string[];
  /** Whether to enable */
  enabled?: boolean;
}

/**
 * Tool Options
 */
export interface ToolOptions {
  /** Timeout period (in milliseconds) */
  timeout?: number;
  /** Maximum number of retries */
  retries?: number;
  /** Retry Delay (in milliseconds) */
  retryDelay?: number;
  /** Whether to enable exponential backoff */
  exponentialBackoff?: boolean;
  /** Maximum number of retries (alias) */
  maxRetries?: number;
  /** Whether to enable logging */
  enableLogging?: boolean;
}

/**
 * Tool test results
 */
export interface ToolTestResult {
  /** Tool ID */
  toolId: string;
  /** Tool Name */
  toolName?: string;
  /** Test whether it was successful. */
  success: boolean;
  /** Test results */
  result?: unknown;
  /** Error message */
  error?: string;
  /** Execution time (in milliseconds) */
  executionTime: number;
  /** Test timestamp */
  timestamp: Timestamp;
}

/**
 * ToolRegistryAPI - Tool Resource Management API
 *
 * Improvements:
 * - Inherits from GenericResourceAPI to reduce duplicate code
 * - Unified cache management
 * - Unified error handling
 * - Unified filtering logic
 * - Maintains backward compatibility
 */
export class ToolRegistryAPI extends CrudResourceAPI<Tool, string, ToolFilter> {
  private dependencies: APIDependencyManager;

  constructor(dependencies: APIDependencyManager) {
    super();
    this.dependencies = dependencies;
  }

  /**
   * Get a single tool
   * @param id: Tool ID
   * @returns: Tool definition; returns null if the tool does not exist
   */
  protected async getResource(id: string): Promise<Tool | null> {
    try {
      return this.dependencies.getToolService().getTool(id);
    } catch (error) {
      if (error instanceof NotFoundError) {
        return null;
      }
      throw error;
    }
  }

  /**
   * Get all tools
   * @returns Array of tool definitions
   */
  protected async getAllResources(): Promise<Tool[]> {
    return this.dependencies.getToolService().listTools();
  }

  /**
   * Create/Register Tool
   * @param tool Tool Definition
   */
  protected async createResource(tool: Tool): Promise<void> {
    this.dependencies.getToolService().registerTool(tool);
  }

  /**
   * Update Tool
   * @param id Tool name
   * @param updates Update content
   */
  protected async updateResource(id: string, updates: Partial<Tool>): Promise<void> {
    // Get the existing tools
    const existingTool = await this.getResource(id);
    if (!existingTool) {
      throw new ToolNotFoundError(`Tool '${id}' not found`, id);
    }

    // Merge and update
    const updatedTool = { ...existingTool, ...updates };

    // Delete the old tool first.
    await this.deleteResource(id);

    // Re-register the new tool.
    await this.createResource(updatedTool);
  }

  /**
   * Delete Tool
   * @param id Tool ID
   */
  protected async deleteResource(id: string): Promise<void> {
    this.dependencies.getToolService().unregisterTool(id);
  }

  /**
   * Clear all tools
   */
  protected override async clearResources(): Promise<void> {
    this.dependencies.getToolService().clear();
  }

  /**
   * Apply filter criteria
   * @param tools Array of tools
   * @param filter Filter criteria
   * @returns Array of tools after filtering
   */
  protected override applyFilter(tools: Tool[], filter: ToolFilter): Tool[] {
    return tools.filter(tool => {
      if (filter.ids && !filter.ids.some(id => tool.id.includes(id))) {
        return false;
      }
      if (filter.name && !tool.name.includes(filter.name)) {
        return false;
      }
      if (filter.type && tool.type !== filter.type) {
        return false;
      }
      if (filter.category && tool.metadata?.category !== filter.category) {
        return false;
      }
      if (filter.tags && tool.metadata?.tags) {
        if (!filter.tags.every(tag => tool.metadata?.tags?.includes(tag))) {
          return false;
        }
      }
      // Enabled filtering is temporarily not supported because the Tool interface has no enabled field
      // TODO: If enabled filtering is needed, need to get the tool's enabled status from elsewhere
      return true;
    });
  }

  /**
   * Validation Tool Definition
   * @param tool: Tool definition
   * @param context: Validation context
   * @returns: Validation result
   */
  protected override async validateResource(
    tool: Tool,
  ): Promise<{ valid: boolean; errors: string[] }> {
    const errors: string[] = [];

    // Use a simplified validation tool to verify the required fields.
    const requiredResult = validateRequiredFields(tool, ["id", "type", "description"], "tool");
    if (requiredResult.isErr()) {
      errors.push(...requiredResult.unwrapOrElse(err => err.map(error => error.message)));
    }

    // Verify the parameter object
    const objectResult = validateObject(tool.parameters, "Tool parameters");
    if (objectResult.isErr()) {
      errors.push(...objectResult.unwrapOrElse(err => err.map(error => error.message)));
    }

    // Verify the ID length
    if (tool.id) {
      const idResult = validateStringLength(tool.id, "Tool ID", 1, 100);
      if (idResult.isErr()) {
        errors.push(...idResult.unwrapOrElse(err => err.map(error => error.message)));
      }
    }

    // Verify the description length.
    if (tool.description) {
      const descriptionResult = validateStringLength(tool.description, "Tool Description", 1, 500);
      if (descriptionResult.isErr()) {
        errors.push(...descriptionResult.unwrapOrElse(err => err.map(error => error.message)));
      }
    }

    return {
      valid: errors.length === 0,
      errors,
    };
  }

  /**
   * Search Tool
   * @param query Search keyword
   * @returns Array defined by the tool
   */
  async searchTools(query: string): Promise<Tool[]> {
    return this.dependencies.getToolService().searchTools(query);
  }

  /**
   * Verify tool parameters
   * @param toolId: Tool ID
   * @param parameters: Tool parameters
   * @returns: Verification result
   */
  async validateToolParameters(
    toolId: string,
    parameters: Record<string, unknown>,
  ): Promise<{ valid: boolean; errors: string[] }> {
    return this.dependencies.getToolService().validateParameters(toolId, parameters);
  }

  /**
   * Get the underlying ToolRegistry instance
   * @returns ToolRegistry instance
   */
  getService() {
    return this.dependencies.getToolService();
  }
}
