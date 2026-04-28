/**
 * Function Registry
 * Manages functions for stateless tools, supporting features such as version control and call statistics.
 */

import type { FunctionRegistryItem, FunctionRegistryConfig } from "../types.js";
import type { ID, ToolOutput } from "@wf-agent/types";
import { ToolError } from "@wf-agent/types";

/**
 * Function Registry
 */
export class FunctionRegistry {
  private functions: Map<string, FunctionRegistryItem> = new Map();
  private config: FunctionRegistryConfig;

  constructor(config: Partial<FunctionRegistryConfig> = {}) {
    this.config = {
      enableVersionControl: config.enableVersionControl ?? true,
      enableCallStatistics: config.enableCallStatistics ?? true,
      maxFunctions: config.maxFunctions ?? 100,
    };
  }

  /**
   * Register function
   */
  register(
    toolId: ID,
    execute: (parameters: Record<string, unknown>) => Promise<ToolOutput>,
    version?: string,
    description?: string,
  ): void {
    // Check the function count limit.
    if (this.functions.size >= this.config.maxFunctions) {
      throw new ToolError(
        `Maximum functions (${this.config.maxFunctions}) reached`,
        toolId,
        "STATELESS",
        { currentFunctions: this.functions.size },
      );
    }

    // Check if it already exists.
    if (this.functions.has(toolId)) {
      if (this.config.enableVersionControl) {
        // If version control is enabled, it allows overwriting existing files.
        // eslint-disable-next-line no-console
        console.warn(`Function '${toolId}' already registered, overwriting...`);
      } else {
        throw new ToolError(`Function '${toolId}' is already registered`, toolId, "STATELESS", {
          toolId,
        });
      }
    }

    // Create a registry key
    const item: FunctionRegistryItem = {
      execute,
      version,
      description,
      registeredAt: new Date(),
      callCount: 0,
    };

    this.functions.set(toolId, item);
  }

  /**
   * Get the function
   */
  get(toolId: ID): FunctionRegistryItem | null {
    return this.functions.get(toolId) || null;
  }

  /**
   * Check if the function exists.
   */
  has(toolId: ID): boolean {
    return this.functions.has(toolId);
  }

  /**
   * Logout function
   */
  unregister(toolId: ID): boolean {
    return this.functions.delete(toolId);
  }

  /**
   * Execute the function
   */
  async execute(toolId: ID, parameters: Record<string, unknown>): Promise<ToolOutput> {
    const item = this.functions.get(toolId);

    if (!item) {
      throw new ToolError(`Function '${toolId}' is not registered`, toolId, "STATELESS", {
        toolId,
      });
    }

    try {
      const result = await item.execute(parameters);

      // Update call statistics
      if (this.config.enableCallStatistics) {
        item.callCount++;
        item.lastCalledAt = new Date();
      }

      return result;
    } catch (error) {
      throw new ToolError(
        `Function execution failed: ${error instanceof Error ? error.message : "Unknown error"}`,
        toolId,
        "STATELESS",
        { parameters },
        error instanceof Error ? error : undefined,
      );
    }
  }

  /**
   * Get all function names
   */
  getFunctionNames(): string[] {
    return Array.from(this.functions.keys());
  }

  /**
   * Get all function information
   */
  getAllFunctionInfo(): Map<string, FunctionRegistryItem> {
    return new Map(this.functions);
  }

  /**
   * Obtain function statistics information
   */
  getFunctionStats(toolId: ID): {
    callCount: number;
    lastCalledAt?: Date;
    registeredAt: Date;
  } | null {
    const item = this.functions.get(toolId);
    if (!item) {
      return null;
    }

    return {
      callCount: item.callCount,
      lastCalledAt: item.lastCalledAt,
      registeredAt: item.registeredAt,
    };
  }

  /**
   * Get the number of registered functions
   */
  getFunctionCount(): number {
    return this.functions.size;
  }

  /**
   * Clear the registry.
   */
  clear(): void {
    this.functions.clear();
  }

  /**
   * Batch registration function
   */
  registerBatch(
    functions: Record<
      string,
      {
        execute: (parameters: Record<string, unknown>) => Promise<ToolOutput>;
        version?: string;
        description?: string;
      }
    >,
  ): void {
    for (const [toolId, func] of Object.entries(functions)) {
      this.register(toolId, func.execute, func.version, func.description);
    }
  }
}
